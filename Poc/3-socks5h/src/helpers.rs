//! Shared helpers used by the `local_node`/`mid_node`/`exit_node` roles
//! defined in `main.rs`: CLI validation, genuine SOCKS5 TCP/UDP capture
//! tunneling, and SOCKS5-in-SOCKS5 relaying.
//!
//! There is no custom envelope any more: every byte that crosses the wire
//! between nodes is genuine SOCKS5. A raw TCP capture becomes a real SOCKS5
//! CONNECT; a raw UDP capture becomes a real SOCKS5 UDP ASSOCIATE. The SOCKS5
//! command itself (`TCPConnect` vs `UDPAssociate`) is what tells `exit_node`
//! which kind of capture it is receiving, so no tagged header is needed.

use fast_socks5::client::{Config as ClientConfig, Socks5Datagram, Socks5Stream};
use fast_socks5::server::{Config as ServerConfig, Socks5Socket};
use fast_socks5::util::target_addr::TargetAddr;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

/// Fixed placeholder CONNECT/UDP-ASSOCIATE target used by `local_node`'s
/// TCP/UDP capture paths. `.invalid` is reserved by RFC 6761 to never
/// resolve, which is why this is safe to use even though `exit_node` (with
/// `dns_resolve` disabled) never actually tries to resolve or dial it: it
/// is purely a self-documenting marker `exit_node` uses to recognize "this
/// CONNECT/UDP-ASSOCIATE is a capture, not a real destination".
pub(crate) const CAPTURE_TARGET_HOST: &str = "datura-capture.invalid";
/// Port paired with `CAPTURE_TARGET_HOST`.
pub(crate) const CAPTURE_TARGET_PORT: u16 = 1;

/// The canonical 10-byte SOCKS5 CONNECT success reply (version, reply code
/// 0x00 = succeeded, reserved, address type IPv4, 4 zero address bytes, 2
/// zero port bytes). Used everywhere a hop needs to hand-write a CONNECT
/// success reply itself because `execute_command` has been disabled on the
/// inbound `Socks5Socket` (so the library no longer writes any reply on our
/// behalf).
pub(crate) const SOCKS5_CONNECT_SUCCESS_REPLY: [u8; 10] =
    [0x05, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

/// Requires `next_hop_address` to be present for the given `role`, printing
/// a loud error and exiting the process otherwise.
pub(crate) fn require_next_hop(next_hop_address: Option<String>, role: &str) -> String {
    next_hop_address.unwrap_or_else(|| {
        eprintln!("--next-hop <host:port> is required for role {role}");
        std::process::exit(1);
    })
}

/// Sends a raw captured TCP `payload` as a genuine SOCKS5 CONNECT through
/// `next_hop_address`, targeting the fixed `CAPTURE_TARGET_HOST`:
/// `CAPTURE_TARGET_PORT` placeholder. Once the handshake succeeds, the raw
/// payload bytes are written directly onto the resulting stream and the
/// connection is then closed (dropped), which signals EOF to whichever hop
/// ultimately reads it (only `exit_node` interprets `CAPTURE_TARGET_HOST` as
/// a capture; intermediate `mid_node` hops just relay the bytes through).
pub(crate) async fn capture_tcp(
    next_hop_address: String,
    payload: Vec<u8>,
) -> fast_socks5::Result<()> {
    let mut socks5_stream = Socks5Stream::connect(
        next_hop_address,
        CAPTURE_TARGET_HOST.to_string(),
        CAPTURE_TARGET_PORT,
        ClientConfig::default(),
    )
    .await?;

    socks5_stream.write_all(&payload).await?;

    println!("tunnel out: {:?}", String::from_utf8_lossy(&payload));

    Ok(())
}

/// Sends a raw captured UDP `payload` as a single datagram over a genuine
/// SOCKS5 UDP ASSOCIATE performed directly against `next_hop_address`. Note
/// that `mid_node` never enables UDP ASSOCIATE support (it stays
/// CONNECT/TCP-only, a deliberate out-of-scope limitation for this pass): if
/// `next_hop_address` points at a `mid_node` instead of `exit_node` directly,
/// the ASSOCIATE request will fail with `CommandNotSupported`.
pub(crate) async fn capture_udp(
    next_hop_address: String,
    payload: Vec<u8>,
) -> fast_socks5::Result<()> {
    let backing_stream = TcpStream::connect(next_hop_address).await?;
    let datagram_socket = Socks5Datagram::bind(backing_stream, "0.0.0.0:0").await?;

    datagram_socket
        .send_to(&payload, (CAPTURE_TARGET_HOST, CAPTURE_TARGET_PORT))
        .await?;

    println!("tunnel out: {:?}", String::from_utf8_lossy(&payload));

    Ok(())
}

/// Case Y (and the entirety of `mid_node`'s per-connection handling):
/// terminates `inbound_stream`'s SOCKS5 handshake as a SOCKS5 server
/// (learning the negotiated `target_host:target_port`, whether that is a
/// genuine client's real destination or `local_node`'s capture placeholder
/// passed along by an upstream hop), then opens its own outer SOCKS5 CONNECT
/// tunnel through `next_hop_address` targeting that same target
/// (SOCKS5-in-SOCKS5 chaining), and bidirectionally copies bytes between the
/// inbound connection and the tunnel to the next hop. This hop itself never
/// dials the real destination; only `exit_node`, at the end of the chain,
/// interprets the target (see `exit_node` in `main.rs`), so this is
/// display-only in the sense that the ultimate destination is never actually
/// reached, only each hop-to-hop link is real.
///
/// The inbound `Socks5Socket` is configured with `execute_command(false)`
/// and `dns_resolve(false)`: without this, `upgrade_to_socks5()` would
/// itself attempt a real outbound dial to the negotiated target (or fail
/// resolving `CAPTURE_TARGET_HOST`, an RFC 6761 `.invalid` domain that can
/// never resolve) before this function's own code ever ran, and would block
/// inside the library's own `transfer()` for as long as that (unwanted)
/// connection stayed open. Because `execute_command` is disabled, the
/// library also never writes a reply on the inbound side any more, so this
/// function must hand-write the CONNECT success reply itself before
/// relaying -- otherwise the inbound caller's own reply read would hang
/// forever.
pub(crate) async fn relay_socks5(
    inbound_stream: TcpStream,
    next_hop_address: String,
) -> fast_socks5::Result<()> {
    let mut inbound_server_config = ServerConfig::default();
    inbound_server_config.set_execute_command(false);
    inbound_server_config.set_dns_resolve(false);
    let inbound_server_config: Arc<ServerConfig> = Arc::new(inbound_server_config);

    let socks5_socket = Socks5Socket::new(inbound_stream, inbound_server_config);
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

    // `execute_command` is disabled above, so nothing has written a reply to
    // the inbound side yet; write it by hand before relaying.
    socks5_socket
        .write_all(&SOCKS5_CONNECT_SUCCESS_REPLY)
        .await?;

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
