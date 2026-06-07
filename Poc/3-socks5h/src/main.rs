use fast_socks5::client::{Config, Socks5Stream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut port: Option<u16> = None;
    let mut proxy: Option<String> = None;
    let mut proxy_port: Option<u16> = None;
    let mut remote_host: Option<String> = None;
    let mut remote_port: Option<u16> = None;

    for pair in args.windows(2) {
        match pair[0].as_str() {
            "--port" => port = Some(pair[1].parse().expect("invalid --port")),
            "--proxy" => proxy = Some(pair[1].clone()),
            "--proxy-port" => proxy_port = Some(pair[1].parse().expect("invalid --proxy-port")),
            "--remote-host" => remote_host = Some(pair[1].clone()),
            "--remote-port" => remote_port = Some(pair[1].parse().expect("invalid --remote-port")),
            _ => {}
        }
    }

    let port = port.unwrap_or_else(|| {
        eprintln!("--port is required");
        std::process::exit(1);
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    match (proxy, proxy_port, remote_host, remote_port) {
        (Some(p), Some(pp), Some(rh), Some(rp)) => {
            let proxy_addr = format!("{}:{}", p, pp);
            runtime.block_on(entry_node(port, proxy_addr, rh, rp));
        }
        (None, None, None, None) => {
            runtime.block_on(mid_exit_node(port));
        }
        _ => {
            eprintln!("entry node requires --port, --proxy, --proxy-port, --remote-host, --remote-port");
            eprintln!("mid/exit node requires only --port");
            std::process::exit(1);
        }
    }
}

async fn entry_node(port: u16, proxy_addr: String, remote_host: String, remote_port: u16) {
    let bind_ip = format!("127.0.0.1:{}", port);

    let udp = UdpSocket::bind(bind_ip.clone())
        .await
        .expect("failed to bind UDP");
    let tcp_listener = TcpListener::bind(bind_ip.clone())
        .await
        .expect("failed to bind TCP");

    // UDP logic
    let proxy_clone = proxy_addr.clone();
    let host_clone  = remote_host.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 65535];

        loop {
            let (amt, _src) = udp.recv_from(&mut buf).await.unwrap();
            println!("UDP out: {:?}", String::from_utf8_lossy(&buf[..amt]));

            let payload = buf[..amt].to_vec();
            let proxy   = proxy_clone.clone();
            let host    = host_clone.clone();

            tokio::spawn(async move {
                if let Err(e) = tunnel(proxy, host, remote_port, payload).await {
                    eprintln!("tunnel error: {e}");
                }
            });
        }
    });

    // TCP Logic
    loop {
        let (mut stream, peer) = tcp_listener.accept().await.unwrap();
        let proxy = proxy_addr.clone();
        let host  = remote_host.clone();

        tokio::spawn(async move {

            let mut tcp_buffer = [0u8; 65535];

            match stream.read(&mut tcp_buffer).await {
                Ok(n) if n > 0 => {
                    let payload = tcp_buffer[..n].to_vec();

                    println!("TCP out: {:?}", String::from_utf8_lossy(&payload));
                    if let Err(e) = tunnel(proxy, host, remote_port, payload).await {
                        eprintln!("tunnel error: {e}");
                    }
                }
                Ok(_)  => println!("TCP out to {})", peer),
                Err(e) => eprintln!("TCP error: {e}"),
            }
        });
    }
}

async fn tunnel(proxy: String, host: String, port: u16, payload: Vec<u8>) -> fast_socks5::Result<()> {
    let mut tcp = Socks5Stream::connect(proxy, host, port, Config::default()).await?;

    tcp.write_all(&(payload.len() as u32).to_be_bytes()).await?;
    tcp.write_all(&payload).await?;
    
    println!("tunnel out: {:?}", String::from_utf8_lossy(&payload));

    let reply_len = tcp.read_u32().await? as usize;
    if reply_len > 0 {
        let mut reply = vec![0u8; reply_len];
        tcp.read_exact(&mut reply).await?;
        println!("sent: {}", String::from_utf8_lossy(&reply));
    }

    Ok(())
}

async fn mid_exit_node(port: u16) {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .expect("failed to bind TCP");

    loop {
        let (mut stream, _peer) = listener.accept().await.unwrap();

        println!("TCP through");

        tokio::spawn(async move {
            let msg_len = match stream.read_u32().await {
                Ok(n)  => n as usize,
                Err(e) => { eprintln!("error in strem creation: {e}"); return; }
            };

            let mut payload = vec![0u8; msg_len];
            if let Err(e) = stream.read_exact(&mut payload).await {
                eprintln!("error with payload: {e}");
                return;
            }

            println!("sent through: {}", String::from_utf8_lossy(&payload));

            // Handles reply to remove tokio package err, you can remove to see end of file error
            let _ = stream.write_all(&0u32.to_be_bytes()).await;
        });
    }
}
