//! PoC 3.1: hidden-service-aware SOCKS5 relaying, built directly on top of
//! PoC 3's genuine-SOCKS5 chain architecture (`local_node -> mid_node* ->
//! exit_node`, see `Poc/3-socks5h`).
//!
//! This PoC adds two things on top of that base:
//!
//! 1. Address classification (`addressing::classify_address`): the entry hop
//!    (`local_node`'s Path C, i.e. genuine inbound SOCKS5 client traffic)
//!    classifies every CONNECT target into one of `.dn` / `.onion` / `.i2p`
//!    / clearnet / public IP / local-or-LAN IP / unknown, logs the
//!    classification, and refuses (with a real SOCKS5
//!    `NOT_ALLOWED_BY_RULESET` reply) anything local/LAN or unclassifiable.
//!    Everything else is still just relayed through the chain and logged at
//!    `exit_node` -- no real circuit differentiation by address type is
//!    implemented yet, only logging and the local/LAN refusal.
//! 2. A static, hardcoded `.dn` hidden-service map
//!    (`addressing::build_hidden_service_map`), consulted only by
//!    `exit_node`, standing in for a `/etc/hosts`-style direct, circuit-free
//!    resolution step: a CONNECT for a mapped `.dn` name is actually dialed
//!    (unlike every other passthrough target, which is only logged) against
//!    the loopback address of a `hidden_service` role instance simulating
//!    that hidden service.
//!
//! ## Roles
//!
//! Four roles are selected via `--role` in `main`:
//!
//! - `local_node`, `mid_node`: unchanged in shape from PoC 3 (see their doc
//!   comments below); `local_node`'s Path C now applies
//!   `helpers::RelayPolicy::ClassifyAndFilter` and `mid_node` applies
//!   `helpers::RelayPolicy::Transparent`.
//! - `exit_node`: as in PoC 3, terminates the inbound SOCKS5 handshake and
//!   inspects the negotiated command/target, but its `TCPConnect` handling
//!   now has three cases instead of two: the capture placeholder (unchanged
//!   from PoC 3), a hit in the static hidden-service map (dial the mapped
//!   loopback address for real and bidirectionally relay), or anything else
//!   (passthrough logging only, unchanged from PoC 3).
//! - `hidden_service`: new. A plain TCP (not SOCKS5) responder simulating a
//!   hidden service: reads one request, always writes back the same fixed
//!   HTTP-ish response body regardless of what it received, and closes.

mod addressing;
mod helpers;

use fast_socks5::Socks5Command;
use fast_socks5::server::{Config as ServerConfig, Socks5Socket};
use fast_socks5::util::target_addr::TargetAddr;
use helpers::{
    CAPTURE_TARGET_HOST, CAPTURE_TARGET_PORT, RelayPolicy, SOCKS5_CONNECT_SUCCESS_REPLY,
    capture_tcp, capture_udp, relay_socks5, require_next_hop, socks5_connect_reply,
};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

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
        Some("hidden_service") => {
            if next_hop_address.is_some() {
                eprintln!("--next-hop is forbidden for hidden_service");
                std::process::exit(1);
            }
            if mid_listen_port.is_some() {
                eprintln!("--mid-port is only valid for --role local_node");
                std::process::exit(1);
            }
            runtime.block_on(hidden_service(listen_port));
        }
        _ => {
            eprintln!("--role must be one of: local_node | mid_node | exit_node | hidden_service");
            std::process::exit(1);
        }
    }
}

/// `local_node` role: binds one local UDP socket and one local TCP listener
/// on `listen_port`. Every UDP datagram (Path A) is relayed to
/// `next_hop_address` as a genuine SOCKS5 UDP ASSOCIATE capture (see
/// `helpers::capture_udp`), and every raw TCP connection (Path B, i.e. a
/// TCP connection whose first byte is not `0x05`) is relayed as a genuine
/// SOCKS5 CONNECT capture (see `helpers::capture_tcp`); both are Case X.
/// Every TCP connection that looks like a genuine SOCKS5 client handshake
/// (first byte `0x05`, Path C) is instead chained through to
/// `next_hop_address` via real SOCKS5-in-SOCKS5 relay (see
/// `helpers::relay_socks5`), applying `RelayPolicy::ClassifyAndFilter` since
/// this is the entry hop, the only place address classification and
/// local/LAN refusal happens; that is Case Y.
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
    // next hop as a genuine SOCKS5 UDP ASSOCIATE capture (Case X).
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
                if let Err(tunnel_error) = capture_udp(next_hop_for_tunnel, payload).await {
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
                // Path C: genuine inbound SOCKS5 client. This is the entry
                // hop, so classification/local-LAN filtering happens here.
                if let Err(relay_error) =
                    relay_socks5(tcp_stream, next_hop_for_tcp, RelayPolicy::ClassifyAndFilter).await
                {
                    eprintln!("relay error: {relay_error}");
                }
            } else {
                // Path B: raw TCP, relay as a genuine SOCKS5 CONNECT capture
                // (Case X).
                let mut tcp_stream = tcp_stream;
                let mut tcp_receive_buffer = [0u8; 65535];

                match tcp_stream.read(&mut tcp_receive_buffer).await {
                    Ok(bytes_read) if bytes_read > 0 => {
                        let payload = tcp_receive_buffer[..bytes_read].to_vec();

                        println!("TCP out: {:?}", String::from_utf8_lossy(&payload));
                        if let Err(tunnel_error) = capture_tcp(next_hop_for_tcp, payload).await {
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
/// socket), relays it to `next_hop_address` via `helpers::relay_socks5`
/// under `RelayPolicy::Transparent` -- a transit hop has no basis on which
/// to classify or refuse a target itself, only the entry hop
/// (`local_node`'s Path C) does that. It has none of `local_node`'s Path
/// A/B capture-construction logic; it is a pure, transparent
/// SOCKS5-to-SOCKS5 relay that never dials the negotiated target itself,
/// only re-negotiates the same CONNECT one hop downstream. It never enables
/// UDP ASSOCIATE support, so it only ever relays TCPConnect traffic. Zero or
/// more `mid_node` hops may sit between `local_node` and `exit_node`.
async fn mid_node(listen_port: u16, next_hop_address: String) {
    let tcp_listener = TcpListener::bind(format!("127.0.0.1:{}", listen_port))
        .await
        .expect("failed to bind TCP");

    loop {
        let (tcp_stream, _peer_address) = tcp_listener.accept().await.unwrap();
        let next_hop_address = next_hop_address.clone();

        tokio::spawn(async move {
            if let Err(relay_error) =
                relay_socks5(tcp_stream, next_hop_address, RelayPolicy::Transparent).await
            {
                eprintln!("relay error: {relay_error}");
            }
        });
    }
}

/// `exit_node` role: binds a TCP listener on `listen_port` and runs a pure
/// SOCKS5 server. For every accepted connection, terminates the inbound
/// SOCKS5 handshake (with `execute_command` disabled -- see module docs --
/// so it never auto-dials or auto-replies) and inspects the negotiated
/// command/target:
///
/// - `UDPAssociate` (always a capture; there is no genuine external UDP
///   client path in this PoC): bind a fresh local UDP socket, hand-write a
///   UDP-ASSOCIATE success reply pointing at it, then receive one datagram,
///   strip its RFC 1928 UDP request header via `fast_socks5::
///   parse_udp_request`, and print the recovered payload. This is Case X.
/// - `TCPConnect` targeting `CAPTURE_TARGET_HOST`:`CAPTURE_TARGET_PORT`:
///   hand-write a CONNECT success reply, then read the raw payload bytes
///   that follow until EOF and print them. This is also Case X.
/// - `TCPConnect` with a target found in the static hidden-service map
///   (see `addressing::build_hidden_service_map`): actually dial the
///   mapped loopback `SocketAddr` for real and bidirectionally relay bytes
///   between the inbound SOCKS5 stream and that upstream connection. This
///   is the one case in this whole PoC where `exit_node` really reaches a
///   destination rather than only logging it.
/// - `TCPConnect` targeting anything else: hand-write a CONNECT success
///   reply, then print the target and drain/discard whatever bytes arrive
///   until EOF. This is Case Y (passthrough mode).
///
/// Aside from the hidden-service-map case above, `exit_node` never dials any
/// real destination; it only prints. It has no successor hop.
async fn exit_node(listen_port: u16) {
    let tcp_listener = TcpListener::bind(format!("127.0.0.1:{}", listen_port))
        .await
        .expect("failed to bind TCP");

    let mut server_config = ServerConfig::default();
    // Without this, `upgrade_to_socks5()` would itself try to really dial
    // the negotiated target (or fail resolving the `.invalid` capture
    // placeholder) before any of the code below ever ran.
    server_config.set_execute_command(false);
    // The capture placeholder domain must reach us unresolved so we can
    // recognize it; a real DNS lookup against an RFC 6761 `.invalid` domain
    // would otherwise fail the handshake outright.
    server_config.set_dns_resolve(false);
    server_config.set_udp_support(true);
    let server_config: Arc<ServerConfig> = Arc::new(server_config);

    // Built once, outside the accept loop, and cheaply `Arc`-cloned into
    // each spawned per-connection task, since the map itself never changes
    // at runtime (it is a hardcoded, `/etc/hosts`-style static table).
    let hidden_service_map = Arc::new(addressing::build_hidden_service_map());

    loop {
        let (tcp_stream, _peer_address) = tcp_listener.accept().await.unwrap();
        let server_config = server_config.clone();
        let hidden_service_map = hidden_service_map.clone();

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

            // Extracted as plain booleans (rather than matching directly on
            // `socks5_socket.cmd()` inline) so no borrow of `socks5_socket`
            // is still alive once the match arms below need to mutate it
            // (e.g. via `write_all`) -- `Socks5Command` isn't `Clone`/`Copy`,
            // and a match scrutinee's temporary borrow is otherwise kept
            // alive for the whole match expression.
            let is_tcp_connect = matches!(socks5_socket.cmd(), Some(Socks5Command::TCPConnect));
            let is_udp_associate = matches!(socks5_socket.cmd(), Some(Socks5Command::UDPAssociate));

            match (is_tcp_connect, is_udp_associate) {
                (true, _) => {
                    let is_capture_target =
                        target_host == CAPTURE_TARGET_HOST && target_port == CAPTURE_TARGET_PORT;
                    let address_type = addressing::classify_address(&target_host);

                    if is_capture_target {
                        // Case X: TCP capture. No length prefix any more --
                        // the sender simply closes the connection once it
                        // has written everything, so read to EOF.
                        if let Err(write_error) =
                            socks5_socket.write_all(&SOCKS5_CONNECT_SUCCESS_REPLY).await
                        {
                            eprintln!("CONNECT reply write error: {write_error}");
                            return;
                        }

                        let mut payload = Vec::new();
                        if let Err(read_error) = socks5_socket.read_to_end(&mut payload).await {
                            eprintln!("TCP capture read error: {read_error}");
                            return;
                        }

                        println!(
                            "TCP reconstructed ({} bytes): {:?}",
                            payload.len(),
                            String::from_utf8_lossy(&payload)
                        );
                    } else if let Some(mapped_address) =
                        addressing::resolve_hidden_service(&hidden_service_map, &target_host)
                    {
                        // New: static hidden-service map hit. Really dial
                        // the simulated hidden service, no circuit involved.
                        println!(
                            "[resolve] {}:{} -> {} (static hidden-service map)",
                            target_host, target_port, mapped_address
                        );

                        match TcpStream::connect(mapped_address).await {
                            Ok(mut upstream_stream) => {
                                if let Err(write_error) =
                                    socks5_socket.write_all(&SOCKS5_CONNECT_SUCCESS_REPLY).await
                                {
                                    eprintln!("CONNECT reply write error: {write_error}");
                                    return;
                                }

                                println!(
                                    "[exit] dialed hidden service {} for {} ({:?})",
                                    mapped_address, target_host, address_type
                                );

                                match tokio::io::copy_bidirectional(
                                    &mut socks5_socket,
                                    &mut upstream_stream,
                                )
                                .await
                                {
                                    Ok((bytes_up, bytes_down)) => println!(
                                        "[exit] hidden-service stream closed ({bytes_up} up / {bytes_down} down)"
                                    ),
                                    Err(copy_error) => {
                                        eprintln!("[exit] hidden-service relay error: {copy_error}")
                                    }
                                }
                            }
                            Err(dial_error) => {
                                let _ = socks5_socket
                                    .write_all(&socks5_connect_reply(
                                        helpers::REPLY_CONNECTION_REFUSED,
                                    ))
                                    .await;
                                eprintln!(
                                    "[exit] hidden service {mapped_address} unreachable: {dial_error}"
                                );
                            }
                        }
                    } else {
                        // Case Y: passthrough mode. No real destination is
                        // ever dialed; just drain/discard whatever arrives
                        // until EOF.
                        if let Err(write_error) =
                            socks5_socket.write_all(&SOCKS5_CONNECT_SUCCESS_REPLY).await
                        {
                            eprintln!("CONNECT reply write error: {write_error}");
                            return;
                        }

                        println!(
                            "SOCKS5 passthrough -> {}:{} ({:?}) [not routed: no circuit for this address type yet]",
                            target_host, target_port, address_type
                        );

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
                }
                (false, true) => {
                    // Case X: UDP capture. Bind a fresh UDP socket to
                    // receive exactly one datagram on, and tell the client
                    // where it is via a hand-written UDP-ASSOCIATE reply.
                    let udp_socket = match UdpSocket::bind("127.0.0.1:0").await {
                        Ok(socket) => socket,
                        Err(bind_error) => {
                            eprintln!("UDP capture bind error: {bind_error}");
                            return;
                        }
                    };

                    let bound_address = match udp_socket.local_addr() {
                        Ok(address) => address,
                        Err(local_addr_error) => {
                            eprintln!("UDP capture local_addr error: {local_addr_error}");
                            return;
                        }
                    };

                    let bound_ipv4_octets = match bound_address.ip() {
                        IpAddr::V4(v4) => v4.octets(),
                        IpAddr::V6(_) => {
                            eprintln!("UDP capture socket unexpectedly bound to an IPv6 address");
                            return;
                        }
                    };

                    let mut udp_associate_reply = vec![0x05, 0x00, 0x00, 0x01];
                    udp_associate_reply.extend_from_slice(&bound_ipv4_octets);
                    udp_associate_reply.extend_from_slice(&bound_address.port().to_be_bytes());

                    if let Err(write_error) = socks5_socket.write_all(&udp_associate_reply).await {
                        eprintln!("UDP ASSOCIATE reply write error: {write_error}");
                        return;
                    }

                    let mut receive_buffer = [0u8; 65535];
                    let (bytes_received, _source_address) =
                        match udp_socket.recv_from(&mut receive_buffer).await {
                            Ok(result) => result,
                            Err(recv_error) => {
                                eprintln!("UDP capture recv error: {recv_error}");
                                return;
                            }
                        };

                    let (fragment_number, _target_addr, payload) =
                        match fast_socks5::parse_udp_request(&receive_buffer[..bytes_received])
                            .await
                        {
                            Ok(parsed) => parsed,
                            Err(parse_error) => {
                                eprintln!("UDP capture parse error: {parse_error}");
                                return;
                            }
                        };

                    if fragment_number != 0 {
                        eprintln!("discarding fragmented UDP capture datagram");
                        return;
                    }

                    println!(
                        "UDP reconstructed ({} bytes): {:?}",
                        payload.len(),
                        String::from_utf8_lossy(payload)
                    );
                }
                (false, false) => {
                    eprintln!("inbound SOCKS5 handshake produced an unsupported command");
                }
            }
        });
    }
}

/// `hidden_service` role: a plain TCP (not SOCKS5) responder simulating a
/// hidden service reachable through `exit_node`'s static hidden-service map.
/// Binds a TCP listener on `listen_port` and, for every accepted connection,
/// reads whatever request bytes arrive in a single `read()` call (not
/// `read_to_end`: a real HTTP client such as `curl` keeps its write side
/// open waiting for a response, so reading to EOF first would deadlock
/// forever), logs them, and always writes back the same fixed HTTP-ish
/// response body regardless of what was received, then closes.
async fn hidden_service(listen_port: u16) {
    let listener = TcpListener::bind(format!("127.0.0.1:{listen_port}"))
        .await
        .expect("failed to bind hidden_service TCP listener");

    loop {
        let (mut stream, peer_address) = listener.accept().await.unwrap();

        tokio::spawn(async move {
            let mut buffer = vec![0u8; 65536];
            match stream.read(&mut buffer).await {
                Ok(bytes_read) if bytes_read > 0 => {
                    println!(
                        "[hidden-service] request from {peer_address} ({bytes_read} bytes): {:?}",
                        String::from_utf8_lossy(&buffer[..bytes_read])
                    );
                }
                _ => {
                    println!("[hidden-service] connection from {peer_address} closed with no data")
                }
            }

            let response_body = b"hello from hiddenserviceajshhsbdbdbdb.dn\n";
            let response = format!(
                "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n",
                response_body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.write_all(response_body).await;
            let _ = stream.shutdown().await;
        });
    }
}
