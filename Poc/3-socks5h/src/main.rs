//! PoC 3: SOCKS5-relayed UDP/TCP forwarding through a chain of SOCKS5 nodes.
//!
//! This PoC demonstrates two things multiplexed onto a single `local_node`
//! listener:
//!
//! 1. Wrapping raw UDP and TCP payloads that `local_node` captures locally
//!    into a tagged envelope, sent through the chain inside a SOCKS5 CONNECT
//!    tunnel that targets a reserved sentinel address (`0.0.0.0:0`). This is
//!    "Case X".
//! 2. Chaining a genuine local SOCKS5 client's CONNECT request through the
//!    chain via a second, outer SOCKS5 CONNECT that targets the client's
//!    real requested destination (SOCKS5-in-SOCKS5). This is "Case Y".
//!
//! There is no more external `dante` proxy: every hop is a pure
//! `fast_socks5::server`-based SOCKS5 server. Each hop terminates the
//! inbound SOCKS5 handshake itself and inspects the negotiated CONNECT
//! target to decide what to do with it:
//!
//! - target == `0.0.0.0:0` (the sentinel) -> envelope mode (Case X). Only
//!   `exit_node` interprets this: it reads the tagged envelope (see below)
//!   and prints what was reconstructed.
//! - any other target -> passthrough mode (Case Y). Only `exit_node`
//!   interprets this: it prints the target and drain/discards whatever
//!   bytes arrive. No real destination is ever dialed and no reply bytes
//!   are ever written back; this PoC remains display-only, with no return
//!   path to the original UDP/TCP/SOCKS5 client.
//!
//! ## Envelope wire format (Case X only)
//!
//! ```text
//! Offset  Size   Field       Value / meaning
//! 0       1      VERSION     0x01
//! 1       1      TRANSPORT   0x00 = raw UDP, 0x01 = raw TCP
//! 2       4      PAYLOAD_LEN u32 big-endian
//! 6       N      PAYLOAD     raw captured bytes
//! ```
//!
//! ## Roles
//!
//! Three roles are selected via `--role` in `main`, forming a chain
//! `local_node -> mid_node* -> exit_node`:
//!
//! - `local_node`: binds a single local UDP socket and a single local TCP
//!   listener on `--port` (9051 by default in typical usage, though
//!   `--port` is always required explicitly). Every UDP datagram and every
//!   raw TCP connection is relayed to `--next-hop` via the tagged envelope
//!   (Case X). Every TCP connection that looks like a genuine SOCKS5 client
//!   handshake (first byte `0x05`) is instead chained through to
//!   `--next-hop` via real SOCKS5-in-SOCKS5 (Case Y).
//! - `mid_node`: optional, and may be repeated zero or more times between
//!   `local_node` and `exit_node` to form an arbitrarily long chain. It is
//!   envelope-blind: for every accepted connection it terminates the
//!   inbound SOCKS5 handshake, learns the negotiated target (sentinel or
//!   real, it does not care which), re-negotiates an identical outer SOCKS5
//!   CONNECT to that same target one hop downstream at `--next-hop`, and
//!   bidirectionally copies bytes between the two connections. It never
//!   reads or interprets the envelope.
//! - `exit_node`: binds a TCP listener on `--port` and runs a SOCKS5 server
//!   that only ever prints what it receives; it never dials any real
//!   destination and has no successor (`--next-hop` is forbidden for this
//!   role).

mod helpers;

use fast_socks5::server::{Config as ServerConfig, Socks5Socket};
use fast_socks5::util::target_addr::TargetAddr;
use helpers::{
    ENVELOPE_VERSION, SENTINEL_PORT, TRANSPORT_TCP, TRANSPORT_UDP, relay_socks5, require_next_hop,
    tunnel_envelope,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, UdpSocket};

fn main() {
    let command_line_arguments: Vec<String> = std::env::args().collect();

    let mut role: Option<String> = None;
    let mut listen_port: Option<u16> = None;
    let mut next_hop_address: Option<String> = None;
    let mut mid_listen_port: Option<u16> = None;

    // Simple flag parser: walks consecutive pairs of args looking for
    // "--flag value" combinations.
    for argument_pair in command_line_arguments.windows(2) {
        match argument_pair[0].as_str() {
            "--role" => role = Some(argument_pair[1].clone()),
            "--port" => listen_port = Some(argument_pair[1].parse().expect("invalid --port")),
            "--next-hop" => next_hop_address = Some(argument_pair[1].clone()),
            "--mid-port" => {
                mid_listen_port = Some(argument_pair[1].parse().expect("invalid --mid-port"))
            }
            _ => {}
        }
    }

    let listen_port = listen_port.unwrap_or_else(|| {
        eprintln!("--port is required (e.g. --port 9051)");
        std::process::exit(1);
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    match role.as_deref() {
        Some("local_node") => {
            let next_hop_address = require_next_hop(next_hop_address, "local_node");
            if mid_listen_port == Some(listen_port) {
                eprintln!(
                    "--mid-port must differ from --port (cannot bind twice on the same port)"
                );
                std::process::exit(1);
            }
            runtime.block_on(local_node(listen_port, next_hop_address, mid_listen_port));
        }
        Some("mid_node") => {
            if mid_listen_port.is_some() {
                eprintln!("--mid-port is only valid for --role local_node");
                std::process::exit(1);
            }
            let next_hop_address = require_next_hop(next_hop_address, "mid_node");
            runtime.block_on(mid_node(listen_port, next_hop_address));
        }
        Some("exit_node") => {
            if next_hop_address.is_some() {
                eprintln!("--next-hop is forbidden for exit_node");
                std::process::exit(1);
            }
            if mid_listen_port.is_some() {
                eprintln!("--mid-port is only valid for --role local_node");
                std::process::exit(1);
            }
            runtime.block_on(exit_node(listen_port));
        }
        _ => {
            eprintln!("--role must be one of: local_node | mid_node | exit_node");
            std::process::exit(1);
        }
    }
}

/// `local_node` role: binds one local UDP socket and one local TCP listener
/// on `listen_port`. Every UDP datagram (Path A) and every raw TCP
/// connection (Path B, i.e. a TCP connection whose first byte is not
/// `0x05`) is relayed to `next_hop_address` via the Case X envelope (see
/// `helpers::tunnel_envelope`). Every TCP connection that looks like a
/// genuine SOCKS5 client handshake (first byte `0x05`, Path C) is instead
/// chained through to `next_hop_address` via real SOCKS5-in-SOCKS5 relay
/// (see `helpers::relay_socks5`).
async fn local_node(listen_port: u16, next_hop_address: String, mid_port: Option<u16>) {
    // Opt-in co-hosting: also run a mid_node SOCKS5 relay listener in this
    // same process on `mid_port`, sharing `next_hop_address`. Spawned before
    // this node's own UDP/TCP entry-node loops so both roles run concurrently.
    if let Some(mid_port) = mid_port {
        let mid_next_hop = next_hop_address.clone();
        tokio::spawn(mid_node(mid_port, mid_next_hop));
    }

    let bind_address = format!("127.0.0.1:{}", listen_port);

    let udp_socket = UdpSocket::bind(bind_address.clone())
        .await
        .expect("failed to bind UDP");
    let tcp_listener = TcpListener::bind(bind_address.clone())
        .await
        .expect("failed to bind TCP");

    // Path A: raw UDP. Read datagrams in a loop and relay each one to the
    // next hop as a Case X envelope.
    let next_hop_for_udp = next_hop_address.clone();
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
            let next_hop_for_tunnel = next_hop_for_udp.clone();

            tokio::spawn(async move {
                if let Err(tunnel_error) =
                    tunnel_envelope(next_hop_for_tunnel, TRANSPORT_UDP, payload).await
                {
                    eprintln!("tunnel error: {tunnel_error}");
                }
            });
        }
    });

    // Path B / Path C: TCP. Accept connections in a loop, peek the first
    // byte to decide whether this is a genuine SOCKS5 client (Path C) or
    // raw TCP to be relayed as an envelope (Path B).
    loop {
        let (tcp_stream, peer_address) = tcp_listener.accept().await.unwrap();
        let next_hop_for_tcp = next_hop_address.clone();

        tokio::spawn(async move {
            let mut first_byte = [0u8; 1];
            let peeked_bytes = match tcp_stream.peek(&mut first_byte).await {
                Ok(count) => count,
                Err(peek_error) => {
                    eprintln!("TCP peek error: {peek_error}");
                    return;
                }
            };

            if peeked_bytes == 0 {
                println!("TCP out to {} (empty connection)", peer_address);
                return;
            }

            if first_byte[0] == 0x05 {
                // Path C: genuine inbound SOCKS5 client.
                if let Err(relay_error) = relay_socks5(tcp_stream, next_hop_for_tcp).await {
                    eprintln!("relay error: {relay_error}");
                }
            } else {
                // Path B: raw TCP, relay as a Case X envelope.
                let mut tcp_stream = tcp_stream;
                let mut tcp_receive_buffer = [0u8; 65535];

                match tcp_stream.read(&mut tcp_receive_buffer).await {
                    Ok(bytes_read) if bytes_read > 0 => {
                        let payload = tcp_receive_buffer[..bytes_read].to_vec();

                        println!("TCP out: {:?}", String::from_utf8_lossy(&payload));
                        if let Err(tunnel_error) =
                            tunnel_envelope(next_hop_for_tcp, TRANSPORT_TCP, payload).await
                        {
                            eprintln!("tunnel error: {tunnel_error}");
                        }
                    }
                    Ok(_) => println!("TCP out to {} (empty payload)", peer_address),
                    Err(read_error) => eprintln!("TCP error: {read_error}"),
                }
            }
        });
    }
}

/// `mid_node` role: binds a TCP listener on `listen_port` and, for every
/// accepted connection (unconditionally, no first-byte peek, no UDP
/// socket), relays it to `next_hop_address` via `helpers::relay_socks5`. It
/// has none of `local_node`'s Path A/B envelope-construction logic and
/// never parses an envelope itself; it is a pure, transparent,
/// envelope-blind SOCKS5 relay. Zero or more `mid_node` hops may sit
/// between `local_node` and `exit_node`.
async fn mid_node(listen_port: u16, next_hop_address: String) {
    let tcp_listener = TcpListener::bind(format!("127.0.0.1:{}", listen_port))
        .await
        .expect("failed to bind TCP");

    loop {
        let (tcp_stream, _peer_address) = tcp_listener.accept().await.unwrap();
        let next_hop_address = next_hop_address.clone();

        tokio::spawn(async move {
            if let Err(relay_error) = relay_socks5(tcp_stream, next_hop_address).await {
                eprintln!("relay error: {relay_error}");
            }
        });
    }
}

/// `exit_node` role: binds a TCP listener on `listen_port` and runs a pure
/// SOCKS5 server. For every accepted connection, terminates the inbound
/// SOCKS5 handshake and inspects the negotiated CONNECT target:
///
/// - if it is the sentinel `0.0.0.0:0`, this is Case X (envelope mode): read
///   the tagged envelope and print what was reconstructed.
/// - otherwise, this is Case Y (passthrough mode): print the target and
///   drain/discard whatever bytes arrive until EOF.
///
/// `exit_node` never dials any real destination and never writes a reply;
/// it only prints. It has no successor hop.
async fn exit_node(listen_port: u16) {
    let tcp_listener = TcpListener::bind(format!("127.0.0.1:{}", listen_port))
        .await
        .expect("failed to bind TCP");

    let server_config: Arc<ServerConfig> = Arc::new(ServerConfig::default());

    loop {
        let (tcp_stream, _peer_address) = tcp_listener.accept().await.unwrap();
        let server_config = server_config.clone();

        tokio::spawn(async move {
            let socks5_socket = Socks5Socket::new(tcp_stream, server_config);
            let mut socks5_socket = match socks5_socket.upgrade_to_socks5().await {
                Ok(socket) => socket,
                Err(handshake_error) => {
                    eprintln!("SOCKS5 handshake error: {handshake_error}");
                    return;
                }
            };

            let target_addr = socks5_socket.target_addr().cloned();
            let (target_host, target_port) = match target_addr {
                Some(TargetAddr::Ip(socket_address)) => {
                    (socket_address.ip().to_string(), socket_address.port())
                }
                Some(TargetAddr::Domain(domain, port)) => (domain, port),
                None => {
                    eprintln!("inbound SOCKS5 handshake produced no target address");
                    return;
                }
            };

            let sentinel_address =
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), SENTINEL_PORT);
            let is_sentinel = target_host == sentinel_address.ip().to_string()
                && target_port == sentinel_address.port();

            if is_sentinel {
                // Case X: envelope mode.
                let mut header = [0u8; 6];
                if let Err(read_error) = socks5_socket.read_exact(&mut header).await {
                    eprintln!("envelope header read error: {read_error}");
                    return;
                }

                let version = header[0];
                let transport = header[1];
                let payload_length =
                    u32::from_be_bytes([header[2], header[3], header[4], header[5]]) as usize;

                if version != ENVELOPE_VERSION {
                    eprintln!("unknown envelope version {version}");
                    return;
                }

                let mut payload = vec![0u8; payload_length];
                if let Err(read_error) = socks5_socket.read_exact(&mut payload).await {
                    eprintln!("envelope payload read error: {read_error}");
                    return;
                }

                match transport {
                    TRANSPORT_UDP => println!(
                        "UDP reconstructed ({} bytes): {:?}",
                        payload_length,
                        String::from_utf8_lossy(&payload)
                    ),
                    TRANSPORT_TCP => println!(
                        "TCP reconstructed ({} bytes): {:?}",
                        payload_length,
                        String::from_utf8_lossy(&payload)
                    ),
                    _ => eprintln!("unknown transport tag {transport}"),
                }
            } else {
                // Case Y: passthrough mode. No real destination is ever
                // dialed; just drain/discard whatever arrives until EOF.
                println!("SOCKS5 passthrough -> {}:{}", target_host, target_port);

                let mut scratch_buffer = [0u8; 65535];
                loop {
                    match socks5_socket.read(&mut scratch_buffer).await {
                        Ok(0) => break,
                        Ok(_bytes_read) => continue,
                        Err(read_error) => {
                            eprintln!("passthrough read error: {read_error}");
                            break;
                        }
                    }
                }
            }
        });
    }
}
