//! Integration test: `local_node` co-hosting a `mid_node` relay via
//! `--mid-port`.
//!
//! This test drives the real `socks5` binary as subprocesses (there is no
//! `[lib]` target in this crate, so the roles in `src/main.rs` are only
//! reachable through the compiled binary, not by calling functions
//! directly) and exercises the exact scenario described in the approved
//! architecture change:
//!
//! 1. `local_node_a` runs with both `--port` (its own entry point) and
//!    `--mid-port` (a co-hosted `mid_node` relay sharing the same
//!    `--next-hop`), forwarding toward a shared `exit_node`.
//! 2. Raw UDP, raw TCP, and a genuine SOCKS5 client request are each sent
//!    directly to `local_node_a`'s entry port.
//! 3. An envelope-wrapped SOCKS5 CONNECT-to-sentinel request (replicating
//!    the exact wire format `helpers::tunnel_envelope` produces) is sent
//!    directly to `local_node_a`'s co-hosted mid-port, exercising the
//!    envelope-reconstruction path through the relay listener.
//! 4. A second, independent `local_node_b` is started with its
//!    `--next-hop` pointed at `local_node_a`'s mid-port, and a UDP request
//!    sent into `local_node_b`'s own entry port is traced all the way
//!    through `local_node_a`'s co-hosted relay to `exit_node`.
//!
//! `exit_node`'s captured stdout is asserted against for each step using
//! distinct, greppable payload contents so each assertion can only match
//! its own log line.

use std::io::{Read, Write};
use std::net::{TcpStream, UdpSocket};
use std::process::{Child, Command, Stdio};
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
        let binary_path = env!("CARGO_BIN_EXE_socks5");
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
/// processes on the same host, but sufficient for a single-threaded test
/// run against a small, fixed number of ports.
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
/// line to a shared, mutex-protected buffer that the test can poll. Used
/// so we can capture `exit_node`'s stdout continuously across the whole
/// test while still being able to assert against it at arbitrary points.
fn spawn_line_collector(
    mut reader: impl Read + Send + 'static,
) -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    let captured_lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
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
    captured_lines: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
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

/// Performs a minimal SOCKS5 client handshake (version 5, no-auth) over
/// `stream`, then issues a CONNECT request for `target_host`:`target_port`
/// and reads the server's reply. `target_host` must be an IPv4 dotted
/// address (sufficient for this test's needs; domain-name ATYP is not
/// exercised here).
fn perform_socks5_connect(stream: &mut TcpStream, target_host: [u8; 4], target_port: u16) {
    // Greeting: version 5, 1 auth method offered, method 0x00 (no auth).
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .expect("failed to write SOCKS5 greeting");

    let mut greeting_reply = [0u8; 2];
    stream
        .read_exact(&mut greeting_reply)
        .expect("failed to read SOCKS5 greeting reply");
    assert_eq!(
        greeting_reply[0], 0x05,
        "unexpected SOCKS5 version in reply"
    );
    assert_eq!(
        greeting_reply[1], 0x00,
        "server did not accept no-auth method"
    );

    // CONNECT request: version 5, command 1 (CONNECT), reserved 0x00,
    // address type 1 (IPv4), 4-byte address, 2-byte big-endian port.
    let mut connect_request = vec![0x05, 0x01, 0x00, 0x01];
    connect_request.extend_from_slice(&target_host);
    connect_request.extend_from_slice(&target_port.to_be_bytes());
    stream
        .write_all(&connect_request)
        .expect("failed to write SOCKS5 CONNECT request");

    // Reply: version, reply code, reserved, address type, then a
    // variable-length bound address depending on address type. For IPv4
    // (address type 1) that is 4 address bytes + 2 port bytes.
    let mut connect_reply_header = [0u8; 4];
    stream
        .read_exact(&mut connect_reply_header)
        .expect("failed to read SOCKS5 CONNECT reply header");
    assert_eq!(
        connect_reply_header[1], 0x00,
        "SOCKS5 CONNECT was rejected, reply code {}",
        connect_reply_header[1]
    );

    let bound_address_length = match connect_reply_header[3] {
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
}

/// Replicates `helpers::tunnel_envelope`'s exact wire format: a SOCKS5
/// CONNECT to the reserved sentinel `0.0.0.0:0`, followed by the tagged
/// envelope (version byte, transport tag byte, big-endian u32 payload
/// length, raw payload bytes). Used to send an envelope-wrapped request
/// directly at a mid-port (Case X), since a bare TCP client hitting a
/// mid_node listener must speak SOCKS5 to be relayed at all.
fn send_envelope_wrapped_request(
    target_address: &str,
    transport_tag: u8,
    payload: &[u8],
) -> TcpStream {
    let mut stream = TcpStream::connect(target_address).unwrap_or_else(|connect_error| {
        panic!("failed to connect to {target_address}: {connect_error}")
    });

    perform_socks5_connect(&mut stream, [0, 0, 0, 0], 0);

    stream
        .write_all(&[0x01]) // ENVELOPE_VERSION
        .expect("failed to write envelope version byte");
    stream
        .write_all(&[transport_tag])
        .expect("failed to write envelope transport tag byte");
    stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .expect("failed to write envelope payload length");
    stream
        .write_all(payload)
        .expect("failed to write envelope payload");

    stream
}

const TRANSPORT_TCP: u8 = 0x01;

/// Exercises the full co-hosted `local_node` + `mid_node` scenario end to
/// end against a real `exit_node`, including a second, independent
/// `local_node` chained through the first node's co-hosted mid-port.
#[test]
fn demonstrates_local_node_co_hosting_mid_node_relay() {
    let exit_node_port = pick_free_port();
    let local_node_a_entry_port = pick_free_port();
    let local_node_a_mid_port = pick_free_port();
    let local_node_b_entry_port = pick_free_port();

    let exit_node_address = format!("127.0.0.1:{exit_node_port}");
    let local_node_a_entry_address = format!("127.0.0.1:{local_node_a_entry_port}");
    let local_node_a_mid_address = format!("127.0.0.1:{local_node_a_mid_port}");
    let local_node_b_entry_address = format!("127.0.0.1:{local_node_b_entry_port}");

    // --- start exit_node and capture its stdout ---
    let exit_node_port_argument = exit_node_port.to_string();
    let mut exit_node_guard = ChildProcessGuard::spawn(
        "exit_node",
        &["--role", "exit_node", "--port", &exit_node_port_argument],
    );
    let exit_node_stdout = exit_node_guard.take_stdout();
    let exit_node_captured_lines = spawn_line_collector(exit_node_stdout);
    // Drain exit_node's stderr on a background thread too, exactly as is
    // already done for local_node_a/local_node_b below. If stderr is left
    // un-drained and exit_node (or a library it depends on, e.g.
    // fast_socks5) ever writes enough there to fill the OS pipe buffer, the
    // child process blocks on that write, which can stall its stdout output
    // too -- surfacing as "zero lines captured" on stdout rather than a
    // content mismatch.
    if let Some(stderr_handle) = exit_node_guard.child_process.stderr.take() {
        let _ = spawn_line_collector(stderr_handle);
    }

    wait_until_tcp_port_accepts_connections(&exit_node_address, Duration::from_secs(5));

    // --- start local_node_a: co-hosts a mid_node relay on --mid-port,
    // both forwarding toward exit_node ---
    let local_node_a_entry_port_argument = local_node_a_entry_port.to_string();
    let local_node_a_mid_port_argument = local_node_a_mid_port.to_string();
    let mut local_node_a_guard = ChildProcessGuard::spawn(
        "local_node_a",
        &[
            "--role",
            "local_node",
            "--port",
            &local_node_a_entry_port_argument,
            "--mid-port",
            &local_node_a_mid_port_argument,
            "--next-hop",
            &exit_node_address,
        ],
    );
    // Drain local_node_a's stdout/stderr on background threads so its pipe
    // buffers never fill up and stall the process; we do not need to
    // assert against local_node_a's own output for this test.
    let _ = spawn_line_collector(local_node_a_guard.take_stdout());
    if let Some(stderr_handle) = local_node_a_guard.child_process.stderr.take() {
        let _ = spawn_line_collector(stderr_handle);
    }

    wait_until_tcp_port_accepts_connections(&local_node_a_entry_address, Duration::from_secs(5));
    wait_until_tcp_port_accepts_connections(&local_node_a_mid_address, Duration::from_secs(5));

    // --- step 3: raw UDP request directly to local_node_a's entry port (P1) ---
    let udp_payload_p1 = b"udp-entry-test-P1";
    {
        let udp_socket = UdpSocket::bind("127.0.0.1:0").expect("failed to bind test UDP socket");
        udp_socket
            .send_to(udp_payload_p1, &local_node_a_entry_address)
            .expect("failed to send UDP datagram to local_node_a entry port");
    }
    let udp_p1_line = wait_for_line_containing(
        &exit_node_captured_lines,
        "udp-entry-test-P1",
        Duration::from_secs(5),
    );
    assert!(
        udp_p1_line.starts_with("UDP reconstructed"),
        "expected a 'UDP reconstructed' line for the P1 UDP request, got: {udp_p1_line}"
    );

    // --- step 4: raw TCP request directly to local_node_a's entry port (P1) ---
    let tcp_payload_p1 = b"tcp-entry-test-P1";
    {
        let mut tcp_stream = TcpStream::connect(&local_node_a_entry_address)
            .expect("failed to connect raw TCP to local_node_a entry port");
        tcp_stream
            .write_all(tcp_payload_p1)
            .expect("failed to write raw TCP payload to local_node_a entry port");
        // Half-close-ish: shut down the write side so local_node_a's read
        // sees the payload promptly rather than waiting on more data.
        let _ = tcp_stream.shutdown(std::net::Shutdown::Write);
    }
    let tcp_p1_line = wait_for_line_containing(
        &exit_node_captured_lines,
        "tcp-entry-test-P1",
        Duration::from_secs(5),
    );
    assert!(
        tcp_p1_line.starts_with("TCP reconstructed"),
        "expected a 'TCP reconstructed' line for the P1 TCP request, got: {tcp_p1_line}"
    );

    // --- step 5: genuine SOCKS5 client CONNECT directly to local_node_a's
    // entry port (P1), exercising the 0x05-peek passthrough path (Case Y) ---
    {
        let mut socks5_stream = TcpStream::connect(&local_node_a_entry_address)
            .expect("failed to connect SOCKS5 client to local_node_a entry port");
        // Arbitrary real-looking target distinct from the sentinel so this
        // is unambiguously Case Y passthrough, not Case X envelope mode.
        perform_socks5_connect(&mut socks5_stream, [93, 184, 216, 34], 80);
        let _ = socks5_stream.shutdown(std::net::Shutdown::Both);
    }
    let socks5_passthrough_line = wait_for_line_containing(
        &exit_node_captured_lines,
        "SOCKS5 passthrough -> 93.184.216.34:80",
        Duration::from_secs(5),
    );
    assert!(
        socks5_passthrough_line.starts_with("SOCKS5 passthrough ->"),
        "expected a Case Y passthrough line for the P1 SOCKS5 request, got: {socks5_passthrough_line}"
    );

    // --- step 6: envelope-wrapped SOCKS5 CONNECT-to-sentinel directly to
    // local_node_a's co-hosted mid-port (P2), exercising the
    // envelope-reconstruction path (Case X) through the mid-port.
    // Transport tag choice: TCP (TRANSPORT_TCP), arbitrarily, since either
    // tag exercises the same code path in exit_node symmetrically. ---
    let midport_direct_payload = b"midport-direct-test-P2";
    {
        let envelope_stream = send_envelope_wrapped_request(
            &local_node_a_mid_address,
            TRANSPORT_TCP,
            midport_direct_payload,
        );
        let _ = envelope_stream.shutdown(std::net::Shutdown::Both);
    }
    let midport_direct_line = wait_for_line_containing(
        &exit_node_captured_lines,
        "midport-direct-test-P2",
        Duration::from_secs(5),
    );
    assert!(
        midport_direct_line.starts_with("TCP reconstructed"),
        "expected a 'TCP reconstructed' line for the P2 mid-port envelope request, got: {midport_direct_line}"
    );

    // --- step 7: start local_node_b, an independent second local_node
    // whose --next-hop points at local_node_a's co-hosted mid-port (P2),
    // not at exit_node directly. Send a UDP request into local_node_b's
    // own entry port (P3) and confirm it reaches exit_node through
    // local_node_a's relay. ---
    let local_node_b_entry_port_argument = local_node_b_entry_port.to_string();
    let mut local_node_b_guard = ChildProcessGuard::spawn(
        "local_node_b",
        &[
            "--role",
            "local_node",
            "--port",
            &local_node_b_entry_port_argument,
            "--next-hop",
            &local_node_a_mid_address,
        ],
    );
    let _ = spawn_line_collector(local_node_b_guard.take_stdout());
    if let Some(stderr_handle) = local_node_b_guard.child_process.stderr.take() {
        let _ = spawn_line_collector(stderr_handle);
    }

    wait_until_tcp_port_accepts_connections(&local_node_b_entry_address, Duration::from_secs(5));

    let node_b_payload = b"nodeB-via-midport-test";
    {
        let udp_socket = UdpSocket::bind("127.0.0.1:0").expect("failed to bind test UDP socket");
        udp_socket
            .send_to(node_b_payload, &local_node_b_entry_address)
            .expect("failed to send UDP datagram to local_node_b entry port");
    }
    let node_b_line = wait_for_line_containing(
        &exit_node_captured_lines,
        "nodeB-via-midport-test",
        Duration::from_secs(5),
    );
    assert!(
        node_b_line.starts_with("UDP reconstructed"),
        "expected a 'UDP reconstructed' line for the local_node_b -> local_node_a(mid-port) -> exit_node chain, got: {node_b_line}"
    );

    // Explicit evidence dump for the report; process cleanup happens via
    // ChildProcessGuard's Drop impl for exit_node_guard, local_node_a_guard,
    // and local_node_b_guard regardless of how this test exits.
    eprintln!("--- captured exit_node stdout evidence ---");
    eprintln!("step 3 (UDP -> P1):        {udp_p1_line}");
    eprintln!("step 4 (TCP -> P1):        {tcp_p1_line}");
    eprintln!("step 5 (SOCKS5 -> P1):     {socks5_passthrough_line}");
    eprintln!("step 6 (envelope -> P2):   {midport_direct_line}");
    eprintln!("step 7 (nodeB -> P2 -> exit): {node_b_line}");

    drop(local_node_b_guard);
    drop(local_node_a_guard);
    drop(exit_node_guard);
}
