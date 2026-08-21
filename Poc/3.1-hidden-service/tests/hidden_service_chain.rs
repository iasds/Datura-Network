//! Integration tests for the `local_node -> exit_node -> hidden_service`
//! chain and its supporting CLI validation.
//!
//! Structural template: `Poc/3-socks5h/tests/co_hosted_mid_node.rs`. As
//! there, roles are only reachable through the compiled `hidden-service`
//! binary (no `[lib]` target in this crate), so every scenario here spawns
//! real child processes, sends real bytes at them over real sockets, and
//! asserts on stdout/stderr log lines and/or actual reply bytes read back
//! off the wire.
//!
//! ## Coverage map (see the QA gap list this file was written against)
//!
//! 1. Full `.dn` happy path, with a real byte round-trip from the
//!    `hidden_service` process: `dn_happy_path_round_trips_real_bytes_and_honors_mapped_port`.
//! 2. `ClassifyAndFilter` refusal wire-reply (`REPLY_NOT_ALLOWED_BY_RULESET`,
//!    `0x02`): `classify_and_filter_refuses_local_ip_with_wire_reply` and
//!    `classify_and_filter_refuses_unknown_domain_with_wire_reply`.
//! 3. `hidden_service` role responds correctly: covered by the same test as
//!    (1) -- it is not itself a SOCKS5 endpoint, so it can only be reached
//!    through the chain, and that test asserts both on the bytes the client
//!    receives *and* on `hidden_service`'s own stdout log line for the
//!    request it processed.
//! 4. Hidden-service-unreachable failure path (`REPLY_CONNECTION_REFUSED`,
//!    `0x05`): `hidden_service_unreachable_yields_connection_refused_reply`.
//! 5. `AddressType::is_routable()` per-variant coverage: added directly in
//!    `src/addressing.rs` (`is_routable_holds_for_every_variant`), not here.
//! 6. `build_hidden_service_map()`'s port-override behavior at the real
//!    `exit_node` call site: folded into the (1) happy-path test, which
//!    deliberately requests a port other than the mapped `5001` and only
//!    succeeds if the mapped port was used instead.
//! 7. CLI validation (invalid `--role`, missing required args, etc.):
//!    the `cli_*` tests at the bottom of this file.
//!
//! Also: a regression check that PoC 3's inherited raw TCP/UDP capture
//! paths still work unmodified in this PoC:
//! `raw_tcp_and_udp_capture_paths_still_work`.
//!
//! ## Port-5001 serialization
//!
//! `addressing::build_hidden_service_map()` hardcodes its one entry to
//! `127.0.0.1:5001` -- there is no CLI knob to point it elsewhere. Any test
//! that depends on what is (or is not) listening on `5001` -- the happy
//! path and the unreachable-hidden-service path -- must not run
//! concurrently with each other (`cargo test` runs `#[test]` functions in
//! parallel threads by default within one binary), since a second
//! `hidden_service` instance could never even bind `5001` if one were
//! already running, and the "unreachable" test specifically depends on
//! nothing being bound there. `lock_port_5001()` below serializes the two.
//!
//! ## Explicit non-goal
//!
//! This file deliberately never sends a raw, unsupported SOCKS5 command
//! (e.g. BIND) or a fragmented UDP capture datagram, and never exercises a
//! `TargetAddr`-less handshake at `exit_node`. Those all hit a pre-existing,
//! separately-tracked silent-drop bug pattern (shared with `Poc/3-socks5h`)
//! where the connection is just dropped with an `eprintln!` and no reply --
//! out of scope to fix or to pin down as "correct" behavior here.

use std::io::{Read, Write};
use std::net::{TcpStream, UdpSocket};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Wraps a spawned child process and guarantees it is killed and reaped
/// when the guard is dropped, regardless of whether the test that owns it
/// panics or returns normally (assertion failures included).
struct ChildProcessGuard {
    child_process: Child,
    role_label: &'static str,
}

impl ChildProcessGuard {
    fn spawn(role_label: &'static str, arguments: &[&str]) -> Self {
        let binary_path = env!("CARGO_BIN_EXE_hidden-service");
        let child_process = Command::new(binary_path)
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|spawn_error| panic!("failed to spawn {role_label}: {spawn_error}"));

        ChildProcessGuard {
            child_process,
            role_label,
        }
    }

    fn take_stdout(&mut self) -> std::process::ChildStdout {
        self.child_process
            .stdout
            .take()
            .unwrap_or_else(|| panic!("{} produced no stdout handle", self.role_label))
    }

    fn take_stderr(&mut self) -> std::process::ChildStderr {
        self.child_process
            .stderr
            .take()
            .unwrap_or_else(|| panic!("{} produced no stderr handle", self.role_label))
    }
}

impl Drop for ChildProcessGuard {
    fn drop(&mut self) {
        let _ = self.child_process.kill();
        let _ = self.child_process.wait();
    }
}

/// Picks a free TCP/UDP-usable localhost port by binding an ephemeral TCP
/// listener momentarily and reading back the OS-assigned port, then
/// releasing it immediately. Not perfectly race-free against other
/// processes on the same host, but sufficient here (same approach as
/// `Poc/3-socks5h/tests/co_hosted_mid_node.rs`).
fn pick_free_port() -> u16 {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind ephemeral port");
    listener
        .local_addr()
        .expect("failed to read ephemeral local address")
        .port()
}

/// Repeatedly attempts a TCP connect to `address` until it succeeds or
/// `timeout` elapses, used to wait for a spawned node's listener to be up
/// before sending test traffic at it.
fn wait_until_tcp_port_accepts_connections(address: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if TcpStream::connect(address).is_ok() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {address} to accept TCP connections");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Reads `reader` on a background thread, line by line, appending every
/// line to a shared, mutex-protected buffer that the test can poll.
fn spawn_line_collector(
    mut reader: impl Read + Send + 'static,
) -> std::sync::Arc<Mutex<Vec<String>>> {
    let captured_lines = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_lines_for_thread = captured_lines.clone();

    std::thread::spawn(move || {
        let mut read_buffer = [0u8; 4096];
        let mut pending_bytes: Vec<u8> = Vec::new();

        loop {
            let bytes_read = match reader.read(&mut read_buffer) {
                Ok(0) => break,
                Ok(bytes_read) => bytes_read,
                Err(_) => break,
            };

            pending_bytes.extend_from_slice(&read_buffer[..bytes_read]);

            while let Some(newline_index) = pending_bytes.iter().position(|byte| *byte == b'\n') {
                let completed_line: Vec<u8> = pending_bytes.drain(..=newline_index).collect();
                let completed_line = String::from_utf8_lossy(&completed_line)
                    .trim_end()
                    .to_string();

                let mut lines_guard = captured_lines_for_thread.lock().unwrap();
                lines_guard.push(completed_line);
            }
        }
    });

    captured_lines
}

/// Polls `captured_lines` until a line containing `needle` appears or
/// `timeout` elapses. Returns the matching line so the caller can print it
/// as evidence, or panics with the full captured log on timeout.
fn wait_for_line_containing(
    captured_lines: &std::sync::Arc<Mutex<Vec<String>>>,
    needle: &str,
    timeout: Duration,
) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        {
            let lines_guard = captured_lines.lock().unwrap();
            if let Some(matching_line) = lines_guard.iter().find(|line| line.contains(needle)) {
                return matching_line.clone();
            }
        }
        if Instant::now() >= deadline {
            let lines_guard = captured_lines.lock().unwrap();
            panic!(
                "timed out waiting for a line containing {needle:?}; captured so far:\n{}",
                lines_guard.join("\n")
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Serializes the two tests whose behavior depends on what is (or is not)
/// bound to `127.0.0.1:5001`, the one hardcoded entry in
/// `addressing::build_hidden_service_map()`. See the module doc comment.
fn port_5001_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Acquires `port_5001_lock()`, recovering from mutex poisoning (a previous
/// holder's test panicking) rather than propagating it, since a panic in
/// one of these two tests should not cascade into spuriously failing the
/// other.
fn lock_port_5001() -> std::sync::MutexGuard<'static, ()> {
    port_5001_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// --- minimal hand-rolled SOCKS5 client helpers -----------------------------
//
// Deliberately independent of the `fast_socks5` client API so these tests
// observe exactly the bytes a real SOCKS5 client (e.g. curl
// --socks5-hostname) would see on the wire, with no library-side
// interpretation in between.

/// Performs the version/no-auth negotiation half of a SOCKS5 handshake.
fn socks5_greeting(stream: &mut TcpStream) {
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .expect("failed to write SOCKS5 greeting");

    let mut greeting_reply = [0u8; 2];
    stream
        .read_exact(&mut greeting_reply)
        .expect("failed to read SOCKS5 greeting reply");
    assert_eq!(
        greeting_reply[0], 0x05,
        "unexpected SOCKS5 version in greeting reply"
    );
    assert_eq!(
        greeting_reply[1], 0x00,
        "server did not accept the no-auth method"
    );
}

/// Writes a CONNECT request with an IPv4 (ATYP 0x01) target.
fn write_connect_request_ipv4(stream: &mut TcpStream, target_host: [u8; 4], target_port: u16) {
    let mut connect_request = vec![0x05, 0x01, 0x00, 0x01];
    connect_request.extend_from_slice(&target_host);
    connect_request.extend_from_slice(&target_port.to_be_bytes());
    stream
        .write_all(&connect_request)
        .expect("failed to write SOCKS5 CONNECT request (IPv4)");
}

/// Writes a CONNECT request with a domain-name (ATYP 0x03) target -- the
/// same wire shape a `--socks5-hostname` curl client uses, and the only
/// shape that lets an unresolvable name like a `.dn` address reach the
/// server unresolved for classification/mapping.
fn write_connect_request_domain(stream: &mut TcpStream, target_domain: &str, target_port: u16) {
    let domain_bytes = target_domain.as_bytes();
    assert!(
        domain_bytes.len() <= u8::MAX as usize,
        "test domain too long for a single-byte SOCKS5 domain length"
    );

    let mut connect_request = vec![0x05, 0x01, 0x00, 0x03, domain_bytes.len() as u8];
    connect_request.extend_from_slice(domain_bytes);
    connect_request.extend_from_slice(&target_port.to_be_bytes());
    stream
        .write_all(&connect_request)
        .expect("failed to write SOCKS5 CONNECT request (domain)");
}

/// Reads a full CONNECT reply (header + variable-length bound address +
/// port) and returns just the reply code byte, leaving the stream
/// positioned right after the reply -- i.e. at the start of any relayed
/// application bytes for a successful CONNECT.
fn read_connect_reply_code(stream: &mut TcpStream) -> u8 {
    let mut reply_header = [0u8; 4];
    stream
        .read_exact(&mut reply_header)
        .expect("failed to read SOCKS5 CONNECT reply header");

    let bound_address_length = match reply_header[3] {
        0x01 => 4,  // IPv4
        0x04 => 16, // IPv6
        0x03 => {
            let mut domain_length_byte = [0u8; 1];
            stream
                .read_exact(&mut domain_length_byte)
                .expect("failed to read SOCKS5 domain length byte");
            domain_length_byte[0] as usize
        }
        other_address_type => panic!("unexpected SOCKS5 bound address type {other_address_type}"),
    };
    let mut bound_address_and_port = vec![0u8; bound_address_length + 2];
    stream
        .read_exact(&mut bound_address_and_port)
        .expect("failed to read SOCKS5 CONNECT bound address/port");

    reply_header[1]
}

/// Full IPv4-target CONNECT: greeting + request + reply code.
fn connect_via_socks5_ipv4(stream: &mut TcpStream, target_host: [u8; 4], target_port: u16) -> u8 {
    socks5_greeting(stream);
    write_connect_request_ipv4(stream, target_host, target_port);
    read_connect_reply_code(stream)
}

/// Full domain-target CONNECT: greeting + request + reply code.
fn connect_via_socks5_domain(stream: &mut TcpStream, target_domain: &str, target_port: u16) -> u8 {
    socks5_greeting(stream);
    write_connect_request_domain(stream, target_domain, target_port);
    read_connect_reply_code(stream)
}

/// Reads whatever is available on `stream` for up to `timeout`, accumulating
/// into a `String` (lossily). Used to read the relayed response body after
/// a successful CONNECT, where the peer closes the connection once done
/// rather than sending a length prefix.
fn read_available_as_string(stream: &mut TcpStream, timeout: Duration) -> String {
    stream
        .set_read_timeout(Some(timeout))
        .expect("failed to set read timeout");
    let mut response_bytes = Vec::new();
    let mut read_buffer = [0u8; 4096];
    loop {
        match stream.read(&mut read_buffer) {
            Ok(0) => break,
            Ok(bytes_read) => response_bytes.extend_from_slice(&read_buffer[..bytes_read]),
            Err(io_error)
                if io_error.kind() == std::io::ErrorKind::WouldBlock
                    || io_error.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(io_error) => panic!("unexpected read error: {io_error}"),
        }
    }
    String::from_utf8_lossy(&response_bytes).to_string()
}

// --- role-spawning helpers ---------------------------------------------

fn spawn_exit_node(port: u16) -> ChildProcessGuard {
    let port_argument = port.to_string();
    ChildProcessGuard::spawn(
        "exit_node",
        &["--role", "exit_node", "--port", &port_argument],
    )
}

fn spawn_local_node(port: u16, next_hop_address: &str) -> ChildProcessGuard {
    let port_argument = port.to_string();
    ChildProcessGuard::spawn(
        "local_node",
        &[
            "--role",
            "local_node",
            "--port",
            &port_argument,
            "--next-hop",
            next_hop_address,
        ],
    )
}

fn spawn_hidden_service(port: u16) -> ChildProcessGuard {
    let port_argument = port.to_string();
    ChildProcessGuard::spawn(
        "hidden_service",
        &["--role", "hidden_service", "--port", &port_argument],
    )
}

/// Drains a guard's stdout/stderr on background threads without asserting
/// against them, so their OS pipe buffers never fill up and stall the
/// child process. Mirrors the same precaution taken in
/// `co_hosted_mid_node.rs`.
fn drain_unused_output(guard: &mut ChildProcessGuard) {
    let _ = spawn_line_collector(guard.take_stdout());
    let _ = spawn_line_collector(guard.take_stderr());
}

// --- (1), (3), (6): full .dn happy path + hidden_service role + port override ---

/// Exercises the PoC's headline acceptance criterion end to end: a real
/// SOCKS5 client CONNECTs through `local_node` -> `exit_node` for the
/// mapped `.dn` name, and the response bytes it reads back are the real
/// bytes written by the actual `hidden_service` process (not a canned
/// reply from `exit_node` itself). Also covers:
///
/// - (3): confirms `hidden_service` itself processed the request, via its
///   own stdout log line, not just that *some* bytes came back.
/// - (6): deliberately requests a port (`59999`) other than the mapped
///   `5001` -- this can only succeed if `exit_node` used the
///   hidden-service-map's port instead of the client-requested one.
#[test]
fn dn_happy_path_round_trips_real_bytes_and_honors_mapped_port() {
    let _port_5001_guard = lock_port_5001();

    let mut hidden_service_guard = spawn_hidden_service(5001);
    let hidden_service_stdout = hidden_service_guard.take_stdout();
    let hidden_service_captured_lines = spawn_line_collector(hidden_service_stdout);
    let _ = spawn_line_collector(hidden_service_guard.take_stderr());
    wait_until_tcp_port_accepts_connections("127.0.0.1:5001", Duration::from_secs(5));

    let exit_node_port = pick_free_port();
    let exit_node_address = format!("127.0.0.1:{exit_node_port}");
    let mut exit_node_guard = spawn_exit_node(exit_node_port);
    let exit_node_captured_lines = spawn_line_collector(exit_node_guard.take_stdout());
    let _ = spawn_line_collector(exit_node_guard.take_stderr());
    wait_until_tcp_port_accepts_connections(&exit_node_address, Duration::from_secs(5));

    let local_node_port = pick_free_port();
    let local_node_address = format!("127.0.0.1:{local_node_port}");
    let mut local_node_guard = spawn_local_node(local_node_port, &exit_node_address);
    drain_unused_output(&mut local_node_guard);
    wait_until_tcp_port_accepts_connections(&local_node_address, Duration::from_secs(5));

    let mut client_stream =
        TcpStream::connect(&local_node_address).expect("failed to connect to local_node");
    // Deliberately not port 5001 (the mapped port): only succeeds end to
    // end if exit_node overrides this with the hidden-service map's port.
    let reply_code =
        connect_via_socks5_domain(&mut client_stream, "hiddenserviceajshhsbdbdbdb.dn", 59999);
    assert_eq!(
        reply_code, 0x00,
        "expected CONNECT success for the mapped .dn name, got reply code {reply_code:#04x}"
    );

    client_stream
        .write_all(b"GET / HTTP/1.0\r\n\r\n")
        .expect("failed to write request through the relayed connection");

    let response_body = read_available_as_string(&mut client_stream, Duration::from_secs(5));
    assert!(
        response_body.contains("hello from hiddenserviceajshhsbdbdbdb.dn"),
        "expected the real hidden_service response body, got: {response_body:?}"
    );

    // (6) evidence: exit_node really did resolve+dial via the static map.
    let resolve_line = wait_for_line_containing(
        &exit_node_captured_lines,
        "[resolve] hiddenserviceajshhsbdbdbdb.dn:59999 -> 127.0.0.1:5001",
        Duration::from_secs(5),
    );
    let dial_line = wait_for_line_containing(
        &exit_node_captured_lines,
        "[exit] dialed hidden service 127.0.0.1:5001",
        Duration::from_secs(5),
    );

    // (3) evidence: hidden_service itself actually processed the request.
    let hidden_service_request_line = wait_for_line_containing(
        &hidden_service_captured_lines,
        "[hidden-service] request from",
        Duration::from_secs(5),
    );

    eprintln!("--- dn happy path evidence ---");
    eprintln!("exit_node resolve line: {resolve_line}");
    eprintln!("exit_node dial line:    {dial_line}");
    eprintln!("hidden_service line:    {hidden_service_request_line}");
    eprintln!("client-observed body:   {response_body:?}");

    drop(local_node_guard);
    drop(exit_node_guard);
    drop(hidden_service_guard);
}

// --- (2): ClassifyAndFilter refusal wire-reply ------------------------------

/// A loopback (`LocalIp`) target must be refused at the wire level with a
/// real `REPLY_NOT_ALLOWED_BY_RULESET` (0x02) reply, not just a log line.
/// This refusal happens entirely inside `local_node`'s own `relay_socks5`
/// call, before `next_hop_address` is ever contacted, so no `exit_node` is
/// needed here -- `--next-hop` is still required by the CLI, so a
/// deliberately-unreachable placeholder address is passed.
#[test]
fn classify_and_filter_refuses_local_ip_with_wire_reply() {
    let local_node_port = pick_free_port();
    let local_node_address = format!("127.0.0.1:{local_node_port}");
    // Never actually dialed for a refused target; any syntactically valid
    // host:port placeholder is fine.
    let mut local_node_guard = spawn_local_node(local_node_port, "127.0.0.1:1");
    drain_unused_output(&mut local_node_guard);
    wait_until_tcp_port_accepts_connections(&local_node_address, Duration::from_secs(5));

    let mut client_stream =
        TcpStream::connect(&local_node_address).expect("failed to connect to local_node");
    let reply_code = connect_via_socks5_ipv4(&mut client_stream, [127, 0, 0, 1], 8080);

    assert_eq!(
        reply_code, 0x02,
        "expected REPLY_NOT_ALLOWED_BY_RULESET (0x02) for a LocalIp target, got {reply_code:#04x}"
    );

    drop(local_node_guard);
}

/// An `Unknown`-classified target (no dot, unrecognized TLD) must also be
/// refused at the wire level with `REPLY_NOT_ALLOWED_BY_RULESET` (0x02).
/// Uses a domain-ATYP CONNECT since `Unknown` here is a hostname, not an IP.
#[test]
fn classify_and_filter_refuses_unknown_domain_with_wire_reply() {
    let local_node_port = pick_free_port();
    let local_node_address = format!("127.0.0.1:{local_node_port}");
    let mut local_node_guard = spawn_local_node(local_node_port, "127.0.0.1:1");
    drain_unused_output(&mut local_node_guard);
    wait_until_tcp_port_accepts_connections(&local_node_address, Duration::from_secs(5));

    let mut client_stream =
        TcpStream::connect(&local_node_address).expect("failed to connect to local_node");
    let reply_code = connect_via_socks5_domain(&mut client_stream, "not-a-real-tld-example", 80);

    assert_eq!(
        reply_code, 0x02,
        "expected REPLY_NOT_ALLOWED_BY_RULESET (0x02) for an Unknown target, got {reply_code:#04x}"
    );

    drop(local_node_guard);
}

// --- (4): hidden-service-unreachable failure path ---------------------------

/// When `exit_node` resolves a `.dn` name via the static map but the mapped
/// address is unreachable (nothing listening on `127.0.0.1:5001`), the
/// client must get a real `REPLY_CONNECTION_REFUSED` (0x05) reply.
#[test]
fn hidden_service_unreachable_yields_connection_refused_reply() {
    let _port_5001_guard = lock_port_5001();
    // Deliberately do NOT start a hidden_service instance on 5001.

    let exit_node_port = pick_free_port();
    let exit_node_address = format!("127.0.0.1:{exit_node_port}");
    let mut exit_node_guard = spawn_exit_node(exit_node_port);
    let exit_node_stderr_lines = spawn_line_collector(exit_node_guard.take_stderr());
    let _ = spawn_line_collector(exit_node_guard.take_stdout());
    wait_until_tcp_port_accepts_connections(&exit_node_address, Duration::from_secs(5));

    let local_node_port = pick_free_port();
    let local_node_address = format!("127.0.0.1:{local_node_port}");
    let mut local_node_guard = spawn_local_node(local_node_port, &exit_node_address);
    drain_unused_output(&mut local_node_guard);
    wait_until_tcp_port_accepts_connections(&local_node_address, Duration::from_secs(5));

    let mut client_stream =
        TcpStream::connect(&local_node_address).expect("failed to connect to local_node");
    let reply_code =
        connect_via_socks5_domain(&mut client_stream, "hiddenserviceajshhsbdbdbdb.dn", 80);

    assert_eq!(
        reply_code, 0x05,
        "expected REPLY_CONNECTION_REFUSED (0x05) when the mapped hidden service is unreachable, got {reply_code:#04x}"
    );

    let unreachable_line = wait_for_line_containing(
        &exit_node_stderr_lines,
        "hidden service 127.0.0.1:5001 unreachable",
        Duration::from_secs(5),
    );
    eprintln!("exit_node unreachable-dial evidence: {unreachable_line}");

    drop(local_node_guard);
    drop(exit_node_guard);
}

// --- regression: raw TCP/UDP capture paths (Path A/B) unchanged from PoC 3 ---

/// `local_node`'s raw UDP (Path A) and raw TCP (Path B) capture paths --
/// inherited unmodified from `Poc/3-socks5h` -- must still work: neither the
/// classification/hidden-service-map additions in this PoC touch them, both
/// only touch Path C (genuine SOCKS5 client traffic) and `exit_node`'s
/// `TCPConnect` handling. Mirrors the README's "raw TCP/UDP capture" manual
/// validation section, previously only checked by hand with `nc`.
#[test]
fn raw_tcp_and_udp_capture_paths_still_work() {
    let exit_node_port = pick_free_port();
    let exit_node_address = format!("127.0.0.1:{exit_node_port}");
    let mut exit_node_guard = spawn_exit_node(exit_node_port);
    let exit_node_captured_lines = spawn_line_collector(exit_node_guard.take_stdout());
    let _ = spawn_line_collector(exit_node_guard.take_stderr());
    wait_until_tcp_port_accepts_connections(&exit_node_address, Duration::from_secs(5));

    let local_node_port = pick_free_port();
    let local_node_address = format!("127.0.0.1:{local_node_port}");
    let mut local_node_guard = spawn_local_node(local_node_port, &exit_node_address);
    drain_unused_output(&mut local_node_guard);
    wait_until_tcp_port_accepts_connections(&local_node_address, Duration::from_secs(5));

    // Path A: raw UDP.
    let udp_payload = b"regression-udp-payload";
    {
        let udp_socket = UdpSocket::bind("127.0.0.1:0").expect("failed to bind test UDP socket");
        udp_socket
            .send_to(udp_payload, &local_node_address)
            .expect("failed to send UDP datagram to local_node");
    }
    let udp_line = wait_for_line_containing(
        &exit_node_captured_lines,
        "regression-udp-payload",
        Duration::from_secs(5),
    );
    assert!(
        udp_line.starts_with("UDP reconstructed"),
        "expected a 'UDP reconstructed' line, got: {udp_line}"
    );

    // Path B: raw TCP.
    let tcp_payload = b"regression-tcp-payload";
    {
        let mut tcp_stream =
            TcpStream::connect(&local_node_address).expect("failed to connect raw TCP");
        tcp_stream
            .write_all(tcp_payload)
            .expect("failed to write raw TCP payload");
        let _ = tcp_stream.shutdown(std::net::Shutdown::Write);
    }
    let tcp_line = wait_for_line_containing(
        &exit_node_captured_lines,
        "regression-tcp-payload",
        Duration::from_secs(5),
    );
    assert!(
        tcp_line.starts_with("TCP reconstructed"),
        "expected a 'TCP reconstructed' line, got: {tcp_line}"
    );

    drop(local_node_guard);
    drop(exit_node_guard);
}

// --- (7): CLI validation ----------------------------------------------------

/// Runs the binary directly (no listener ever comes up for any of these
/// cases, so a blocking `.output()` wait is safe and fast) and returns
/// `(exit_code, stdout, stderr)`.
fn run_cli(arguments: &[&str]) -> (i32, String, String) {
    let binary_path = env!("CARGO_BIN_EXE_hidden-service");
    let output = Command::new(binary_path)
        .args(arguments)
        .output()
        .expect("failed to run CLI binary");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn cli_missing_port_produces_clean_error() {
    let (exit_code, _stdout, stderr) = run_cli(&["--role", "local_node"]);
    assert_eq!(exit_code, 1, "stderr was: {stderr}");
    assert!(
        stderr.contains("--port is required"),
        "stderr was: {stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "expected a clean error, not a panic; stderr was: {stderr}"
    );
}

#[test]
fn cli_invalid_role_produces_clean_error() {
    let (exit_code, _stdout, stderr) = run_cli(&["--role", "not_a_real_role", "--port", "9999"]);
    assert_eq!(exit_code, 1, "stderr was: {stderr}");
    assert!(
        stderr.contains("--role must be one of"),
        "stderr was: {stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "expected a clean error, not a panic; stderr was: {stderr}"
    );
}

#[test]
fn cli_missing_next_hop_produces_clean_error_for_local_node() {
    let (exit_code, _stdout, stderr) = run_cli(&["--role", "local_node", "--port", "9999"]);
    assert_eq!(exit_code, 1, "stderr was: {stderr}");
    assert!(
        stderr.contains("--next-hop <host:port> is required for role local_node"),
        "stderr was: {stderr}"
    );
}

#[test]
fn cli_missing_next_hop_produces_clean_error_for_mid_node() {
    let (exit_code, _stdout, stderr) = run_cli(&["--role", "mid_node", "--port", "9999"]);
    assert_eq!(exit_code, 1, "stderr was: {stderr}");
    assert!(
        stderr.contains("--next-hop <host:port> is required for role mid_node"),
        "stderr was: {stderr}"
    );
}

#[test]
fn cli_next_hop_forbidden_for_exit_node() {
    let (exit_code, _stdout, stderr) = run_cli(&[
        "--role",
        "exit_node",
        "--port",
        "9999",
        "--next-hop",
        "127.0.0.1:1",
    ]);
    assert_eq!(exit_code, 1, "stderr was: {stderr}");
    assert!(
        stderr.contains("--next-hop is forbidden for exit_node"),
        "stderr was: {stderr}"
    );
}

#[test]
fn cli_mid_port_forbidden_for_exit_node() {
    let (exit_code, _stdout, stderr) = run_cli(&[
        "--role",
        "exit_node",
        "--port",
        "9999",
        "--mid-port",
        "9998",
    ]);
    assert_eq!(exit_code, 1, "stderr was: {stderr}");
    assert!(
        stderr.contains("--mid-port is only valid for --role local_node"),
        "stderr was: {stderr}"
    );
}

/// KNOWN GAP (see final report): unlike the other CLI validation cases
/// above, a malformed `--port` value does *not* currently produce a clean
/// error -- `main.rs` parses it with `.expect("invalid --port")`, which
/// panics (Rust exit code 101) instead of printing a clean message and
/// exiting 1 like every other validation failure in this file. This test
/// intentionally documents the *actual* current behavior (so a regression
/// -- e.g. the panic message changing shape, or this silently starting to
/// hang instead of exiting -- would be caught) rather than asserting the
/// desired "clean error" behavior, which would just make this test fail.
/// Fixing this is a `main.rs` code change outside this task's scope
/// (writing tests), and is flagged separately in the task report.
#[test]
fn cli_invalid_port_value_currently_panics_instead_of_erroring_cleanly() {
    let (exit_code, _stdout, stderr) = run_cli(&[
        "--role",
        "local_node",
        "--port",
        "not-a-number",
        "--next-hop",
        "127.0.0.1:1",
    ]);
    assert_eq!(
        exit_code, 101,
        "documenting current (buggy) panic behavior; if this now fails, either the panic's \
         exit code changed or (better) the underlying .expect(\"invalid --port\") in main.rs \
         was replaced with a clean error path -- update this test accordingly. stderr was: {stderr}"
    );
    assert!(
        stderr.contains("panicked") && stderr.contains("invalid --port"),
        "stderr was: {stderr}"
    );
}
