//! Shared helpers used by the `local_node`/`mid_node`/`exit_node` roles
//! defined in `main.rs`: CLI validation, Case X envelope tunneling, and
//! Case Y SOCKS5-in-SOCKS5 relaying.

use fast_socks5::client::{Config as ClientConfig, Socks5Stream};
use fast_socks5::server::{Config as ServerConfig, Socks5Socket};
use fast_socks5::util::target_addr::TargetAddr;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

/// Sentinel host used by `local_node`/`mid_node` to tell the next hop "this
/// SOCKS5 CONNECT is not a real destination, what follows (once it reaches
/// `exit_node`) is a Case X envelope".
pub(crate) const SENTINEL_HOST: &str = "0.0.0.0";
/// Sentinel port paired with `SENTINEL_HOST`.
pub(crate) const SENTINEL_PORT: u16 = 0;

/// Envelope format version (see module docs).
pub(crate) const ENVELOPE_VERSION: u8 = 0x01;
/// Envelope transport tag: raw UDP payload.
pub(crate) const TRANSPORT_UDP: u8 = 0x00;
/// Envelope transport tag: raw TCP payload.
pub(crate) const TRANSPORT_TCP: u8 = 0x01;

/// Requires `next_hop_address` to be present for the given `role`, printing
/// a loud error and exiting the process otherwise.
pub(crate) fn require_next_hop(next_hop_address: Option<String>, role: &str) -> String {
    next_hop_address.unwrap_or_else(|| {
        eprintln!("--next-hop <host:port> is required for role {role}");
        std::process::exit(1);
    })
}

/// Case X: opens a SOCKS5 CONNECT tunnel through `next_hop_address`,
/// targeting the reserved sentinel `0.0.0.0:0`, then writes the tagged
/// envelope (`ENVELOPE_VERSION`, `transport_tag`, big-endian u32 length,
/// `payload`). No reply is read back: this PoC has no return path,
/// `exit_node` does not write anything for envelope-mode connections, and
/// the tunnel is simply closed once the envelope has been written.
pub(crate) async fn tunnel_envelope(
    next_hop_address: String,
    transport_tag: u8,
    payload: Vec<u8>,
) -> fast_socks5::Result<()> {
    let mut socks5_stream = Socks5Stream::connect(
        next_hop_address,
        SENTINEL_HOST.to_string(),
        SENTINEL_PORT,
        ClientConfig::default(),
    )
    .await?;

    socks5_stream.write_all(&[ENVELOPE_VERSION]).await?;
    socks5_stream.write_all(&[transport_tag]).await?;
    socks5_stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    socks5_stream.write_all(&payload).await?;

    println!("tunnel out: {:?}", String::from_utf8_lossy(&payload));

    Ok(())
}

/// Case Y (and the entirety of `mid_node`'s per-connection handling):
/// terminates `inbound_stream`'s SOCKS5 handshake as a SOCKS5 server
/// (learning the negotiated `target_host:target_port`, whether that is a
/// genuine client's real destination or the sentinel passed along by an
/// upstream hop), then opens its own outer SOCKS5 CONNECT tunnel through
/// `next_hop_address` targeting that same target (SOCKS5-in-SOCKS5
/// chaining), and bidirectionally copies bytes between the inbound
/// connection and the tunnel to the next hop. This hop itself never dials
/// the real destination; only `exit_node`, at the end of the chain,
/// interprets the target (see `exit_node` in `main.rs`), so this is
/// display-only in the sense that the ultimate destination is never
/// actually reached, only each hop-to-hop link is real.
pub(crate) async fn relay_socks5(
    inbound_stream: TcpStream,
    next_hop_address: String,
) -> fast_socks5::Result<()> {
    let server_config: Arc<ServerConfig> = Arc::new(ServerConfig::default());
    let socks5_socket = Socks5Socket::new(inbound_stream, server_config);
    let mut socks5_socket = socks5_socket.upgrade_to_socks5().await?;

    let target_addr = socks5_socket.target_addr().cloned();
    let (target_host, target_port) = match target_addr {
        Some(TargetAddr::Ip(socket_address)) => {
            (socket_address.ip().to_string(), socket_address.port())
        }
        Some(TargetAddr::Domain(domain, port)) => (domain, port),
        None => {
            eprintln!("local SOCKS5 handshake produced no target address");
            return Ok(());
        }
    };

    println!("SOCKS5 client -> {}:{}", target_host, target_port);

    let mut next_hop_stream = Socks5Stream::connect(
        next_hop_address,
        target_host,
        target_port,
        ClientConfig::default(),
    )
    .await?;

    tokio::io::copy_bidirectional(&mut socks5_socket, &mut next_hop_stream).await?;

    Ok(())
}
