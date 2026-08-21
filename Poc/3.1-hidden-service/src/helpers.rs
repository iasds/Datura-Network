//! Shared helpers used by the `local_node`/`mid_node`/`exit_node`/
//! `hidden_service` roles defined in `main.rs`: CLI validation, genuine
//! SOCKS5 TCP/UDP capture tunneling, and SOCKS5-in-SOCKS5 relaying.
//!
//! There is no custom envelope any more: every byte that crosses the wire
//! between nodes is genuine SOCKS5. A raw TCP capture becomes a real SOCKS5
//! CONNECT; a raw UDP capture becomes a real SOCKS5 UDP ASSOCIATE. The SOCKS5
//! command itself (`TCPConnect` vs `UDPAssociate`) is what tells `exit_node`
//! which kind of capture it is receiving, so no tagged header is needed.

use fast_socks5::SocksError;
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

/// SOCKS5 reply codes used when hand-writing CONNECT replies (see
/// `socks5_connect_reply`).
pub(crate) const REPLY_SUCCEEDED: u8 = 0x00;
pub(crate) const REPLY_GENERAL_FAILURE: u8 = 0x01;
pub(crate) const REPLY_NOT_ALLOWED_BY_RULESET: u8 = 0x02;
pub(crate) const REPLY_CONNECTION_REFUSED: u8 = 0x05;

/// Builds a 10-byte SOCKS5 CONNECT reply (version, `reply_code`, reserved,
/// address type IPv4, 4 zero address bytes, 2 zero port bytes) for the given
/// reply code. Used everywhere a hop needs to hand-write a CONNECT reply
/// itself because `execute_command` has been disabled on the inbound
/// `Socks5Socket` (so the library no longer writes any reply on our behalf).
pub(crate) const fn socks5_connect_reply(reply_code: u8) -> [u8; 10] {
    [
        0x05, reply_code, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]
}

/// The canonical SOCKS5 CONNECT success reply, equivalent to
/// `socks5_connect_reply(REPLY_SUCCEEDED)`.
pub(crate) const SOCKS5_CONNECT_SUCCESS_REPLY: [u8; 10] = socks5_connect_reply(REPLY_SUCCEEDED);

/// Governs what `relay_socks5` does with the negotiated target once it has
/// learned it, before relaying.
pub(crate) enum RelayPolicy {
    /// Entry-hop behavior: classify the target address and refuse (with a
    /// `REPLY_NOT_ALLOWED_BY_RULESET` reply) anything that isn't routable,
    /// per `addressing::AddressType::is_routable`. Used by `local_node`'s
    /// Path C.
    ClassifyAndFilter,
    /// Transit-hop behavior: relay whatever target was negotiated, unjudged.
    /// Used by `mid_node`, which has no basis on which to make a routing
    /// decision of its own -- only the entry hop does.
    Transparent,
}

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
/// `policy` governs what happens once the target is known, before relaying:
/// see `RelayPolicy`. Under `RelayPolicy::ClassifyAndFilter`, a non-routable
/// target causes this function to write a `REPLY_NOT_ALLOWED_BY_RULESET`
/// reply and return early without ever contacting `next_hop_address`.
///
/// The inbound `Socks5Socket` is configured with `execute_command(false)`
/// and `dns_resolve(false)`: without this, `upgrade_to_socks5()` would
/// itself attempt a real outbound dial to the negotiated target (or fail
/// resolving `CAPTURE_TARGET_HOST`, an RFC 6761 `.invalid` domain that can
/// never resolve) before this function's own code ever ran, and would block
/// inside the library's own `transfer()` for as long as that (unwanted)
/// connection stayed open. Because `execute_command` is disabled, the
/// library also never writes a reply on the inbound side any more, so this
/// function must hand-write the CONNECT reply itself before relaying --
/// otherwise the inbound caller's own reply read would hang forever.
///
/// Unlike PoC 3's `relay_socks5` (which writes the client-facing success
/// reply optimistically, before the outer `Socks5Stream::connect` to
/// `next_hop_address` has even been attempted), this reply is written only
/// *after* that connect has actually succeeded. `Socks5Stream::connect`
/// performs the full SOCKS5 client handshake itself, including reading and
/// interpreting the next hop's own CONNECT reply, and returns
/// `Err(SocksError::ReplyError(_))` if that reply was anything other than
/// success -- this is how a downstream failure (e.g. `exit_node` failing to
/// dial a mapped hidden service and writing back a real
/// `REPLY_CONNECTION_REFUSED`) becomes visible here before this hop has
/// committed to telling its own client "success". On such a failure, the
/// real reply code carried by the error is forwarded verbatim to the
/// client; any other connect error (e.g. `next_hop_address` itself being
/// unreachable, which has no SOCKS5 reply code of its own) falls back to
/// `REPLY_GENERAL_FAILURE`.
pub(crate) async fn relay_socks5(
    inbound_stream: TcpStream,
    next_hop_address: String,
    policy: RelayPolicy,
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

    match policy {
        RelayPolicy::Transparent => {
            println!("SOCKS5 client -> {}:{}", target_host, target_port);
        }
        RelayPolicy::ClassifyAndFilter => {
            let address_type = crate::addressing::classify_address(&target_host);
            println!(
                "[classify] {}:{} -> {:?}",
                target_host, target_port, address_type
            );
            if !address_type.is_routable() {
                println!(
                    "[refused] {}:{} ({:?}) - local/LAN targets are excluded from routing",
                    target_host, target_port, address_type
                );
                socks5_socket
                    .write_all(&socks5_connect_reply(REPLY_NOT_ALLOWED_BY_RULESET))
                    .await?;
                return Ok(());
            }
        }
    }

    // `execute_command` is disabled above, so nothing has written a reply to
    // the inbound side yet. Unlike PoC 3, that reply is *not* written here:
    // dial `next_hop_address` first and only report success to the real
    // client once that dial (and everything downstream of it, transitively,
    // since `Socks5Stream::connect` reads the next hop's own reply too) has
    // actually succeeded.
    let mut next_hop_stream = match Socks5Stream::connect(
        next_hop_address,
        target_host,
        target_port,
        ClientConfig::default(),
    )
    .await
    {
        Ok(next_hop_stream) => next_hop_stream,
        Err(connect_error) => {
            let reply_code = match &connect_error {
                SocksError::ReplyError(reply_error) => reply_error.as_u8(),
                _ => REPLY_GENERAL_FAILURE,
            };
            eprintln!("relay to next hop failed: {connect_error}");
            socks5_socket
                .write_all(&socks5_connect_reply(reply_code))
                .await?;
            return Ok(());
        }
    };

    // The next hop (and, transitively, everything downstream of it) really
    // did succeed, so it is now safe to tell the real client so too.
    socks5_socket
        .write_all(&SOCKS5_CONNECT_SUCCESS_REPLY)
        .await?;

    tokio::io::copy_bidirectional(&mut socks5_socket, &mut next_hop_stream).await?;

    Ok(())
}
