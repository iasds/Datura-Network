//! PoC 3: SOCKS5-tunneled UDP/TCP forwarding through a `dante` proxy.
//!
//! This PoC demonstrates wrapping raw UDP and TCP traffic into a SOCKS5 CONNECT
//! tunnel (via `fast_socks5`) and forwarding it to a downstream node. It is
//! intentionally single-hop and display-only: payloads are printed to stdout as
//! they move through the pipeline, and no return path back to the original
//! UDP/TCP client is implemented.
//!
//! Two roles are selected via CLI flags in `main`:
//! - `app`: binds local UDP/TCP listeners, receives traffic, and tunnels each
//!   payload through a SOCKS5 proxy to a remote node.
//! - `node`: binds a TCP listener and receives tunneled payloads forwarded by
//!   `app` (via the proxy), printing them out.

use fast_socks5::client::{Config, Socks5Stream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

fn main() {
    let command_line_arguments: Vec<String> = std::env::args().collect();

    let mut listen_port: Option<u16> = None;
    let mut proxy_host: Option<String> = None;
    let mut proxy_port: Option<u16> = None;
    let mut remote_host: Option<String> = None;
    let mut remote_port: Option<u16> = None;

    // Simple flag parser: walks consecutive pairs of args looking for
    // "--flag value" combinations.
    for argument_pair in command_line_arguments.windows(2) {
        match argument_pair[0].as_str() {
            "--port" => listen_port = Some(argument_pair[1].parse().expect("invalid --port")),
            "--proxy" => proxy_host = Some(argument_pair[1].clone()),
            "--proxy-port" => {
                proxy_port = Some(argument_pair[1].parse().expect("invalid --proxy-port"))
            }
            "--remote-host" => remote_host = Some(argument_pair[1].clone()),
            "--remote-port" => {
                remote_port = Some(argument_pair[1].parse().expect("invalid --remote-port"))
            }
            _ => {}
        }
    }

    let listen_port = listen_port.unwrap_or_else(|| {
        eprintln!("--port is required");
        std::process::exit(1);
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    // If proxy + remote target flags are all present, run as the `app` (entry)
    // role; if none are present, run as the `node` (mid/exit) role. Any partial
    // combination is a usage error.
    match (proxy_host, proxy_port, remote_host, remote_port) {
        (Some(proxy_host), Some(proxy_port), Some(remote_host), Some(remote_port)) => {
            let proxy_address = format!("{}:{}", proxy_host, proxy_port);
            runtime.block_on(app(listen_port, proxy_address, remote_host, remote_port));
        }
        (None, None, None, None) => {
            runtime.block_on(node(listen_port));
        }
        _ => {
            eprintln!("entry node requires --port, --proxy, --proxy-port, --remote-host, --remote-port");
            eprintln!("mid/exit node requires only --port");
            std::process::exit(1);
        }
    }
}

/// Entry-node role: binds local UDP and TCP listeners on `listen_port`, and for
/// every datagram/connection received, forwards the payload through a SOCKS5
/// tunnel (see `tunnel`) to `remote_host:remote_port` via the proxy at
/// `proxy_address`.
async fn app(listen_port: u16, proxy_address: String, remote_host: String, remote_port: u16) {
    let bind_address = format!("127.0.0.1:{}", listen_port);

    let udp_socket = UdpSocket::bind(bind_address.clone())
        .await
        .expect("failed to bind UDP");
    let tcp_listener = TcpListener::bind(bind_address.clone())
        .await
        .expect("failed to bind TCP");

    // UDP logic: read datagrams in a loop and spawn a tunnel task per datagram.
    let proxy_address_for_udp = proxy_address.clone();
    let remote_host_for_udp = remote_host.clone();
    tokio::spawn(async move {
        let mut receive_buffer = [0u8; 65535];

        loop {
            let (bytes_received, _source_address) =
                udp_socket.recv_from(&mut receive_buffer).await.unwrap();
            println!(
                "UDP out: {:?}",
                String::from_utf8_lossy(&receive_buffer[..bytes_received])
            );

            let payload = receive_buffer[..bytes_received].to_vec();
            let proxy_address_for_tunnel = proxy_address_for_udp.clone();
            let remote_host_for_tunnel = remote_host_for_udp.clone();

            tokio::spawn(async move {
                if let Err(tunnel_error) = tunnel(
                    proxy_address_for_tunnel,
                    remote_host_for_tunnel,
                    remote_port,
                    payload,
                )
                .await
                {
                    eprintln!("tunnel error: {tunnel_error}");
                }
            });
        }
    });

    // TCP logic: accept connections in a loop and spawn a tunnel task per
    // connection, tunneling whatever is read from the first `read` call.
    loop {
        let (mut tcp_stream, peer_address) = tcp_listener.accept().await.unwrap();
        let proxy_address_for_tcp = proxy_address.clone();
        let remote_host_for_tcp = remote_host.clone();

        tokio::spawn(async move {
            let mut tcp_receive_buffer = [0u8; 65535];

            match tcp_stream.read(&mut tcp_receive_buffer).await {
                Ok(bytes_read) if bytes_read > 0 => {
                    let payload = tcp_receive_buffer[..bytes_read].to_vec();

                    println!("TCP out: {:?}", String::from_utf8_lossy(&payload));
                    if let Err(tunnel_error) = tunnel(
                        proxy_address_for_tcp,
                        remote_host_for_tcp,
                        remote_port,
                        payload,
                    )
                    .await
                    {
                        eprintln!("tunnel error: {tunnel_error}");
                    }
                }
                Ok(_) => println!("TCP out to {})", peer_address),
                Err(read_error) => eprintln!("TCP error: {read_error}"),
            }
        });
    }
}

/// Opens a SOCKS5 CONNECT tunnel through `proxy_address` to
/// `remote_host:remote_port`, then sends `payload` using a simple
/// length-prefixed framing: a big-endian u32 byte length followed by the raw
/// payload bytes. This framing lets the receiving `node` know exactly how many
/// bytes to read for the message, since SOCKS5/TCP is just a byte stream with
/// no built-in message boundaries.
///
/// After sending, it reads back a u32-prefixed reply. This PoC is display-only
/// with no real return path, so `node` always replies with a zero-length
/// message (see `node`) and no reply bytes are actually read in practice.
async fn tunnel(
    proxy_address: String,
    remote_host: String,
    remote_port: u16,
    payload: Vec<u8>,
) -> fast_socks5::Result<()> {
    let mut socks5_stream =
        Socks5Stream::connect(proxy_address, remote_host, remote_port, Config::default()).await?;

    socks5_stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    socks5_stream.write_all(&payload).await?;

    println!("tunnel out: {:?}", String::from_utf8_lossy(&payload));

    let reply_length = socks5_stream.read_u32().await? as usize;
    if reply_length > 0 {
        let mut reply_buffer = vec![0u8; reply_length];
        socks5_stream.read_exact(&mut reply_buffer).await?;
        println!("sent: {}", String::from_utf8_lossy(&reply_buffer));
    }

    Ok(())
}

/// Mid/exit-node role: binds a TCP listener on `listen_port` and, for each
/// incoming connection, reads one length-prefixed message (u32 big-endian
/// length + payload, matching the framing written by `tunnel`) and prints it.
async fn node(listen_port: u16) {
    let tcp_listener = TcpListener::bind(format!("127.0.0.1:{}", listen_port))
        .await
        .expect("failed to bind TCP");

    loop {
        let (mut tcp_stream, _peer_address) = tcp_listener.accept().await.unwrap();

        println!("TCP through");

        tokio::spawn(async move {
            let payload_length = match tcp_stream.read_u32().await {
                Ok(length) => length as usize,
                Err(read_error) => {
                    eprintln!("error in strem creation: {read_error}");
                    return;
                }
            };

            let mut payload = vec![0u8; payload_length];
            if let Err(read_error) = tcp_stream.read_exact(&mut payload).await {
                eprintln!("error with payload: {read_error}");
                return;
            }

            println!("sent through: {}", String::from_utf8_lossy(&payload));

            // Send back a zero-length reply so `tunnel`'s `read_u32` call
            // completes cleanly. This PoC has no real return path; the
            // zero-length reply exists purely so the write side of the
            // connection is properly finished off instead of erroring out.
            let _ = tcp_stream.write_all(&0u32.to_be_bytes()).await;
        });
    }
}
