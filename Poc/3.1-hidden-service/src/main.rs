use fast_socks5::client::{Config, Socks5Stream};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

#[derive(Debug, Clone, PartialEq)]
enum AddressType {
    DaturaHidden, // *.dn
    TorOnion,     // *.onion
    I2P,          // *.i2p
    Clearnet,     // *.com / *.net / *.org / any other TLD
    PublicIp,     // routable IPv4/IPv6
    LocalIp,      // 127.x, 10.x, 192.168.x, 172.16-31.x, ::1
    Unknown,
}

fn classify_address(host: &str) -> AddressType {
    // Try parsing as an IP address first
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                if v4.is_loopback()
                    || octets[0] == 10
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168)
                {
                    AddressType::LocalIp
                } else {
                    AddressType::PublicIp
                }
            }
            IpAddr::V6(v6) => {
                if v6.is_loopback() {
                    AddressType::LocalIp
                } else {
                    AddressType::PublicIp
                }
            }
        };
    }

    // Hostname-based classification — check TLD
    let lower = host.to_lowercase();
    let tld = lower.rsplit('.').next().unwrap_or("");
    match tld {
        "dn" => AddressType::DaturaHidden,
        "onion" => AddressType::TorOnion,
        "i2p" => AddressType::I2P,
        tld if !tld.is_empty() && lower.contains('.') => AddressType::Clearnet,
        _ => AddressType::Unknown,
    }
}

fn build_dns_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    // Hardcoded for PoC — simulates /etc/hosts for .dn hidden services
    map.insert(
        "hiddenserviceajshhsbdbdbdb.dn".to_string(),
        "127.0.0.1:5001".to_string(),
    );
    map
}

#[allow(dead_code)]
struct ConnectionState {
    peer_addr:   std::net::SocketAddr,
    addr_type:   AddressType,
    target_host: String,
    target_port: u16,
    bytes_sent:  u64,
    bytes_recv:  u64,
    started_at:  Instant,
}

type ConnectionMap = Arc<Mutex<HashMap<std::net::SocketAddr, ConnectionState>>>;

trait Transport {
    async fn send(&mut self, buf: &[u8]) -> std::io::Result<usize>;
    async fn recv(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
}

struct TokioTcpTransport {
    inner: tokio::net::TcpStream,
}

impl Transport for TokioTcpTransport {
    async fn send(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf).await
    }
    async fn recv(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf).await
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut port: Option<u16> = None;
    let mut proxy: Option<String> = None;
    let mut proxy_port: Option<u16> = None;
    let mut remote_host: Option<String> = None;
    let mut remote_port: Option<u16> = None;
    let mut listen_addr: Option<String> = None;

    for pair in args.windows(2) {
        match pair[0].as_str() {
            "--port" => port = Some(pair[1].parse().expect("invalid --port")),
            "--proxy" => proxy = Some(pair[1].clone()),
            "--proxy-port" => proxy_port = Some(pair[1].parse().expect("invalid --proxy-port")),
            "--remote-host" => remote_host = Some(pair[1].clone()),
            "--remote-port" => remote_port = Some(pair[1].parse().expect("invalid --remote-port")),
            "--listen" => listen_addr = Some(pair[1].clone()),
            _ => {}
        }
    }

    let (port, bind_host) = if let Some(ref la) = listen_addr {
        let mut parts = la.rsplitn(2, ':');
        let p: u16 = parts.next().expect("--listen missing port").parse().expect("invalid port in --listen");
        let h = parts.next().unwrap_or("127.0.0.1").to_string();
        (p, h)
    } else {
        let p = port.unwrap_or_else(|| {
            eprintln!("--port or --listen is required");
            std::process::exit(1);
        });
        (p, "127.0.0.1".to_string())
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    match (proxy, proxy_port, remote_host, remote_port) {
        (Some(p), Some(pp), Some(rh), Some(rp)) => {
            let proxy_addr = format!("{}:{}", p, pp);
            let dns_map = Arc::new(build_dns_map());
            let conn_map: ConnectionMap = Arc::new(Mutex::new(HashMap::new()));
            runtime.block_on(entry_node(port, bind_host, proxy_addr, rh, rp, dns_map, conn_map));
        }
        (None, None, None, None) => {
            runtime.block_on(mid_exit_node(port, bind_host));
        }
        _ => {
            eprintln!("entry node requires --port, --proxy, --proxy-port, --remote-host, --remote-port");
            eprintln!("mid/exit node requires only --port");
            std::process::exit(1);
        }
    }
}

async fn entry_node(
    port: u16,
    bind_host: String,
    proxy_addr: String,
    remote_host: String,
    remote_port: u16,
    dns_map: Arc<HashMap<String, String>>,
    conn_map: ConnectionMap,
) {
    let bind_ip = format!("{}:{}", bind_host, port);

    let udp = UdpSocket::bind(bind_ip.clone())
        .await
        .expect("failed to bind UDP");
    let tcp_listener = TcpListener::bind(bind_ip.clone())
        .await
        .expect("failed to bind TCP");

    // UDP logic
    let proxy_clone = proxy_addr.clone();
    let host_clone  = remote_host.clone();
    let dns_map_udp = dns_map.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 65535];

        loop {
            let (amt, src) = udp.recv_from(&mut buf).await.unwrap();

            let addr_type = classify_address(&host_clone);
            println!(
                "[entry] UDP datagram from {} → {} ({:?})",
                src, host_clone, addr_type
            );

            // DNS resolution for .dn addresses
            let (resolved_host, resolved_port) = if addr_type == AddressType::DaturaHidden {
                let key = host_clone.to_lowercase();
                if let Some(target) = dns_map_udp.get(key.as_str()) {
                    let mut parts = target.splitn(2, ':');
                    let h = parts.next().unwrap_or(&host_clone).to_string();
                    let p = parts.next()
                        .and_then(|p| p.parse::<u16>().ok())
                        .unwrap_or(remote_port);
                    println!("[dns] {} → {}:{}", host_clone, h, p);
                    (h, p)
                } else {
                    eprintln!("[dns] no entry for {}, dropping", host_clone);
                    continue;
                }
            } else {
                (host_clone.clone(), remote_port)
            };

            let payload = buf[..amt].to_vec();
            let proxy   = proxy_clone.clone();

            println!("UDP out: {:?}", String::from_utf8_lossy(&buf[..amt]));

            tokio::spawn(async move {
                if let Err(e) = tunnel(proxy, resolved_host, resolved_port, payload).await {
                    eprintln!("tunnel error: {e}");
                }
            });
        }
    });

    // TCP logic
    loop {
        let (mut stream, peer) = tcp_listener.accept().await.unwrap();
        let proxy    = proxy_addr.clone();
        let host     = remote_host.clone();
        let dns_map  = dns_map.clone();
        let conn_map = conn_map.clone();

        tokio::spawn(async move {
            let addr_type = classify_address(&host);
            println!(
                "[entry] TCP connection from {} → {} ({:?})",
                peer, host, addr_type
            );

            // DNS resolution for .dn addresses
            let (resolved_host, resolved_port) = if addr_type == AddressType::DaturaHidden {
                let key = host.to_lowercase();
                if let Some(target) = dns_map.get(key.as_str()) {
                    let mut parts = target.splitn(2, ':');
                    let h = parts.next().unwrap_or(&host).to_string();
                    let p = parts.next()
                        .and_then(|p| p.parse::<u16>().ok())
                        .unwrap_or(remote_port);
                    println!("[dns] {} → {}:{}", host, h, p);
                    (h, p)
                } else {
                    eprintln!("[dns] no entry for {}, dropping", host);
                    return;
                }
            } else {
                (host.clone(), remote_port)
            };

            // Register connection
            {
                let mut map = conn_map.lock().unwrap();
                map.insert(peer, ConnectionState {
                    peer_addr:   peer,
                    addr_type:   addr_type.clone(),
                    target_host: resolved_host.clone(),
                    target_port: resolved_port,
                    bytes_sent:  0,
                    bytes_recv:  0,
                    started_at:  Instant::now(),
                });
                println!(
                    "[conn] opened: {} → {}:{} ({:?})",
                    peer, resolved_host, resolved_port, addr_type
                );
            }

            // Framed read: read_u32 length prefix then read_exact payload
            let msg_len = match stream.read_u32().await {
                Ok(n)  => n as usize,
                Err(e) => { eprintln!("TCP framing error from {peer}: {e}"); return; }
            };
            let mut tcp_buffer = vec![0u8; msg_len];
            match stream.read_exact(&mut tcp_buffer).await {
                Ok(_) => {
                    let payload = tcp_buffer;
                    println!("TCP out: {:?}", String::from_utf8_lossy(&payload));
                    if let Err(e) = tunnel(proxy, resolved_host, resolved_port, payload).await {
                        eprintln!("tunnel error: {e}");
                    }
                }
                Err(e) => eprintln!("TCP read error from {peer}: {e}"),
            }

            // Deregister connection
            {
                let mut map = conn_map.lock().unwrap();
                if let Some(state) = map.remove(&peer) {
                    println!(
                        "[conn] closed: {} after {:.2?}",
                        state.peer_addr,
                        state.started_at.elapsed()
                    );
                }
            }
        });
    }
}

// This is the udp to tcp tunnel
async fn tunnel(proxy: String, host: String, port: u16, payload: Vec<u8>) -> fast_socks5::Result<()> {
    let mut tcp = Socks5Stream::connect(proxy, host, port, Config::default()).await?;

    tcp.write_all(&(payload.len() as u32).to_be_bytes()).await?;
    tcp.write_all(&payload).await?;

    println!("tunnel out: {:?}", String::from_utf8_lossy(&payload));

    const MAX_REPLY: usize = 16 * 1024 * 1024; // 16 MiB sanity cap
    let reply_len = tcp.read_u32().await? as usize;
    if reply_len > 0 {
        if reply_len > MAX_REPLY {
            eprintln!("reply_len {reply_len} exceeds cap {MAX_REPLY}, dropping");
            return Ok(());
        }
        let mut reply = vec![0u8; reply_len];
        tcp.read_exact(&mut reply).await?;
        println!("sent: {}", String::from_utf8_lossy(&reply));
    }

    Ok(())
}

async fn mid_exit_node(port: u16, bind_host: String) {
    let listener = TcpListener::bind(format!("{}:{}", bind_host, port))
        .await
        .expect("failed to bind TCP");

    loop {
        let (mut stream, _peer) = listener.accept().await.unwrap();

        println!("TCP through");

        tokio::spawn(async move {
            let msg_len = match stream.read_u32().await {
                Ok(n)  => n as usize,
                Err(e) => { eprintln!("error in stream creation: {e}"); return; }
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
