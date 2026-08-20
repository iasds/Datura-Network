mod equix_pow;
use equix_pow::*;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const CHALLENGE_DIFFICULTY: u32 = 800;
const HASH_LEN: usize = 32;

// Read/write timeout applied to every connection.
// A peer that connects but stalls will otherwise hold the thread open indefinitely.
const IO_TIMEOUT: Duration = Duration::from_secs(30);

// A packet may be re-addressed and forwarded this max amount before it is dropped.
// Without a limit, nodes can recursively forward the same packet, so a
// single packet becomes unbounded, one thread and one socket per hop.
const MAX_HOPS: u8 = 8;

// Upper bound on routing table. An entry costs one PoW to install but nothing to
// keep, adversary can buy unbounded memory on the node without cap.
const MAX_RULES: usize = 1024;

// Upper bound on concurrent connections. Every connection is a thread
// for up to IO_TIMEOUT.
const MAX_CONNECTIONS: usize = 256;

// Max target address a rule may carry. Both "255.255.255.255:65535" and bracketed IPv6 forms fit.
const MAX_ADDR_LEN: usize = 64;

const MSG_REGISTER: u8 = 1;
const MSG_PACKET: u8 = 2;
const MSG_ACK: u8 = 3;
const MSG_REJECT: u8 = 4;

struct RoutingRule {
    target_addr: SocketAddr,
    target_hash: [u8; HASH_LEN],
}

type RoutingTable = Arc<Mutex<HashMap<[u8; HASH_LEN], RoutingRule>>>;

// Populate a 32-byte hash from an ASCII string by zero-padding.
// "44AWD" becomes [0x34,0x34,0x41,0x57,0x44,0x00,...].
// Eventually this will be the node's hashring position (Blake3 of its .dn address).
fn hash_from_str(s: &str) -> [u8; HASH_LEN] {
    let b = s.as_bytes();
    let mut h = [0u8; HASH_LEN];
    let n = b.len().min(HASH_LEN);
    h[..n].copy_from_slice(&b[..n]);
    h
}

fn hash_display(h: &[u8; HASH_LEN]) -> String {
    let end = h.iter().rposition(|&b| b != 0).map(|i| i + 1).unwrap_or(0);
    if h[..end].iter().all(|&b| b.is_ascii_graphic()) {
        String::from_utf8_lossy(&h[..end]).to_string()
    } else {
        h[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
            + "..."
    }
}

fn read_exact(stream: &mut TcpStream, buf: &mut [u8]) -> bool {
    stream.read_exact(buf).is_ok()
}

// Rejections look identical. Reporting a distinct reason would let
// anyone spend PoW to learn whether a given hash is routed.
fn reject(stream: &mut TcpStream) {
    let _ = stream.write_all(&[MSG_REJECT]);
}

// Decrements the live-connection count when a handler thread ends.
struct ConnectionGuard(Arc<AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

// Shared accept loop. Bounds live handler threads and treats a failed
// accept as recoverable.
fn serve<F>(port: u16, label: &'static str, handler: F)
where
    F: Fn(&mut TcpStream, &str) + Send + Sync + 'static,
{
    let listener = match TcpListener::bind(format!("0.0.0.0:{port}")) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[{label}:{port}] bind failed: {e}");
            return;
        }
    };
    println!("[{label}:{port}] listening");

    let handler = Arc::new(handler);
    let live = Arc::new(AtomicUsize::new(0));

    for incoming in listener.incoming() {
        let mut stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                println!("[{label}] accept failed: {e}");
                continue;
            }
        };

        if live.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            live.fetch_sub(1, Ordering::AcqRel);
            println!("[{label}] at connection limit ({MAX_CONNECTIONS}), dropping");
            continue;
        }
        let guard = ConnectionGuard(live.clone());

        if stream.set_read_timeout(Some(IO_TIMEOUT)).is_err()
            || stream.set_write_timeout(Some(IO_TIMEOUT)).is_err()
        {
            continue;
        }

        let peer = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        let handler = handler.clone();
        let spawned = thread::Builder::new().spawn(move || {
            let _guard = guard;
            (*handler)(&mut stream, &peer);
        });
        if spawned.is_err() {
            println!("[{label}] could not spawn handler thread, dropping");
        }
    }
}

// A rule's target is supplied by the registrant and Node A connects on demand.
// Parsing strictly as an IP:port prevents a DNS lookup on a name of the
// registrant's choosing, and rules out addresses that would have the node send to non-clients.
fn parse_target(addr_bytes: &[u8], self_port: u16) -> Result<SocketAddr, &'static str> {
    let text = std::str::from_utf8(addr_bytes).map_err(|_| "target address is not valid UTF-8")?;
    let addr: SocketAddr = text
        .parse()
        .map_err(|_| "target address is not an IP:port")?;

    if addr.port() == 0 {
        return Err("target port is zero");
    }
    let ip = addr.ip();
    if ip.is_unspecified() || ip.is_multicast() {
        return Err("target address is not a unicast host");
    }
    if let IpAddr::V4(v4) = ip
        && v4.is_broadcast()
    {
        return Err("target address is a broadcast address");
    }
    // A rule pointing back at this node's listener is a loop.
    // MAX_HOPS bounds, but there is no reason to accept.
    if addr.port() == self_port && ip.is_loopback() {
        return Err("target address is this node");
    }
    Ok(addr)
}

fn run_node_a(port: u16) {
    let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
    serve(port, "node-a", move |stream, peer| {
        let mut type_buf = [0u8; 1];
        if !read_exact(stream, &mut type_buf) {
            return;
        }

        match type_buf[0] {
            MSG_REGISTER => handle_register(stream, peer, &table, port),
            MSG_PACKET => handle_packet(stream, peer, &table),
            other => println!("[node-a] {peer}: unknown message type {other}"),
        }
    });
}

fn handle_register(stream: &mut TcpStream, peer: &str, table: &RoutingTable, self_port: u16) {
    let challenge = create_challenge(CHALLENGE_DIFFICULTY);
    if stream.write_all(&challenge.to_le_bytes()).is_err() {
        return;
    }
    println!(
        "[node-a] {peer}: register request, sent challenge (difficulty {CHALLENGE_DIFFICULTY})"
    );

    // Wire format for registration (received after challenge):
    //   [solution: 24 bytes]
    //   [match_hash: 32 bytes]
    //   [target_addr_len: 2 bytes LE]
    //   [target_addr: N bytes]
    //   [target_hash: 32 bytes]

    let mut solution = [0u8; 24];
    if !read_exact(stream, &mut solution) {
        return;
    }

    if !verify_solution(CHALLENGE_DIFFICULTY, challenge, solution) {
        println!("[node-a] {peer}: bad PoW, rejecting");
        reject(stream);
        return;
    }

    let mut match_hash = [0u8; HASH_LEN];
    if !read_exact(stream, &mut match_hash) {
        return;
    }

    let mut addr_len_buf = [0u8; 2];
    if !read_exact(stream, &mut addr_len_buf) {
        return;
    }
    let addr_len = u16::from_le_bytes(addr_len_buf) as usize;
    if addr_len > MAX_ADDR_LEN {
        println!("[node-a] {peer}: target address too long ({addr_len} bytes), rejecting");
        reject(stream);
        return;
    }
    let mut addr_buf = vec![0u8; addr_len];
    if !read_exact(stream, &mut addr_buf) {
        return;
    }

    let mut target_hash = [0u8; HASH_LEN];
    if !read_exact(stream, &mut target_hash) {
        return;
    }

    let target_addr = match parse_target(&addr_buf, self_port) {
        Ok(a) => a,
        Err(why) => {
            println!("[node-a] {peer}: {why}, rejecting");
            reject(stream);
            return;
        }
    };

    // The routing table is never sent; exists only in memory.
    // Peers can only observe that a rule exists by watching packets get forwarded.
    let stored = {
        let mut t = table.lock().unwrap();
        if t.len() >= MAX_RULES {
            Err("routing table full")
        } else {
            match t.entry(match_hash) {
                // First wins. Allowing overwrites would let anyone displace a live rule,
                // and redirecting its traffic to an address of their choosing.
                // No way to tell a legitimate re-registration from a hijack, so refuse both. (PoC 10 scope)
                Entry::Occupied(_) => Err("a rule for that hash already exists"),
                Entry::Vacant(slot) => {
                    slot.insert(RoutingRule {
                        target_addr,
                        target_hash,
                    });
                    Ok(())
                }
            }
        }
    };

    if let Err(why) = stored {
        println!("[node-a] {peer}: {why}, rejecting");
        reject(stream);
        return;
    }

    println!(
        "[node-a] {peer}: rule stored: '{}' -> {} @ '{}'",
        hash_display(&match_hash),
        target_addr,
        hash_display(&target_hash)
    );
    let _ = stream.write_all(&[MSG_ACK]);
}

fn handle_packet(stream: &mut TcpStream, peer: &str, table: &RoutingTable) {
    // Wire format for a packet:
    //   [hops_left: 1 byte]
    //   [dest_hash: 32 bytes]
    //   [payload_len: 2 bytes LE]
    //   [payload: N bytes]

    let mut hops_buf = [0u8; 1];
    if !read_exact(stream, &mut hops_buf) {
        return;
    }
    let hops_left = hops_buf[0];

    let mut dest_hash = [0u8; HASH_LEN];
    if !read_exact(stream, &mut dest_hash) {
        return;
    }

    let mut len_buf = [0u8; 2];
    if !read_exact(stream, &mut len_buf) {
        return;
    }
    let payload_len = u16::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; payload_len];
    if !read_exact(stream, &mut payload) {
        return;
    }

    println!(
        "[node-a] {peer}: packet arrived for hash '{}' ({hops_left} hops left)",
        hash_display(&dest_hash)
    );

    if hops_left == 0 {
        println!("[node-a] {peer}: hop limit reached, dropping");
        return;
    }

    let rule = {
        let t = table.lock().unwrap();
        t.get(&dest_hash).map(|r| (r.target_addr, r.target_hash))
    };

    match rule {
        None => {
            // Unknown hash: drop silently. The sender gets no response either way,
            // so a fail reveals nothing about what rules are stored.
            println!(
                "[node-a] {peer}: no routing rule for '{}', dropping",
                hash_display(&dest_hash)
            );
        }
        Some((target_addr, target_hash)) => {
            println!(
                "[node-a] {peer}: forwarding to {} re-addressed as '{}'",
                target_addr,
                hash_display(&target_hash)
            );
            match TcpStream::connect(target_addr) {
                Err(e) => println!("[node-a] connect to {target_addr} failed: {e}"),
                Ok(mut fwd) => {
                    let ok = fwd.set_write_timeout(Some(IO_TIMEOUT)).is_ok()
                        && fwd.write_all(&[MSG_PACKET]).is_ok()
                        && fwd.write_all(&[hops_left - 1]).is_ok()
                        && fwd.write_all(&target_hash).is_ok()
                        && fwd.write_all(&(payload.len() as u16).to_le_bytes()).is_ok()
                        && fwd.write_all(&payload).is_ok();
                    if ok {
                        println!("[node-a] forwarded {} payload bytes", payload.len());
                    } else {
                        println!("[node-a] forward write failed");
                    }
                }
            }
        }
    }
}

fn run_node_c(port: u16) {
    serve(port, "node-c", |stream, peer| {
        let mut type_buf = [0u8; 1];
        if !read_exact(stream, &mut type_buf) {
            return;
        }
        if type_buf[0] != MSG_PACKET {
            return;
        }

        let mut hops_buf = [0u8; 1];
        if !read_exact(stream, &mut hops_buf) {
            return;
        }

        let mut dest_hash = [0u8; HASH_LEN];
        if !read_exact(stream, &mut dest_hash) {
            return;
        }

        let mut len_buf = [0u8; 2];
        if !read_exact(stream, &mut len_buf) {
            return;
        }
        let payload_len = u16::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; payload_len];
        if !read_exact(stream, &mut payload) {
            return;
        }

        println!("[node-c] packet received from {peer}:");
        println!("  dest_hash : '{}'", hash_display(&dest_hash));
        println!("  payload   : \"{}\"", String::from_utf8_lossy(&payload));
    });
}

fn do_register(node_a_addr: &str, match_hash_str: &str, node_c_addr: &str, target_hash_str: &str) {
    let match_hash = hash_from_str(match_hash_str);
    let target_hash = hash_from_str(target_hash_str);

    println!("[register] connecting to node-a at {node_a_addr}");
    let mut stream = TcpStream::connect(node_a_addr).expect("connect failed");
    stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();

    stream.write_all(&[MSG_REGISTER]).unwrap();

    let mut challenge_buf = [0u8; 16];
    stream.read_exact(&mut challenge_buf).unwrap();
    let challenge = u128::from_le_bytes(challenge_buf);
    println!(
        "[register] challenge received (difficulty {}), solving...",
        get_challenge_effort(challenge)
    );

    let threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let solution = solve_challenge(threads, challenge);
    println!(
        "[register] solved, submitting rule: '{}' -> {} @ '{}'",
        match_hash_str, node_c_addr, target_hash_str
    );

    let addr_bytes = node_c_addr.as_bytes();
    stream.write_all(&solution).unwrap();
    stream.write_all(&match_hash).unwrap();
    stream
        .write_all(&(addr_bytes.len() as u16).to_le_bytes())
        .unwrap();
    stream.write_all(addr_bytes).unwrap();
    stream.write_all(&target_hash).unwrap();

    let mut resp = [0u8; 1];
    stream.read_exact(&mut resp).unwrap();
    match resp[0] {
        MSG_ACK => println!("[register] routing rule accepted"),
        // Node A provides no response. Rejection is identical so that
        // it cannot be used by advisaries.
        MSG_REJECT => println!("[register] routing rule rejected"),
        other => println!("[register] unexpected response byte {other}"),
    }
}

fn do_send(node_a_addr: &str, dest_hash_str: &str, message: &str) {
    let dest_hash = hash_from_str(dest_hash_str);
    let payload = message.as_bytes();

    println!(
        "[send] --> node-a at {node_a_addr}, hash '{}', {} bytes",
        dest_hash_str,
        payload.len()
    );
    let mut stream = TcpStream::connect(node_a_addr).expect("connect failed");
    stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();

    stream.write_all(&[MSG_PACKET]).unwrap();
    stream.write_all(&[MAX_HOPS]).unwrap();
    stream.write_all(&dest_hash).unwrap();
    stream
        .write_all(&(payload.len() as u16).to_le_bytes())
        .unwrap();
    stream.write_all(payload).unwrap();
}

fn run_test() {
    let port_a: u16 = 9110;
    let port_c: u16 = 9111;
    let addr_a = format!("127.0.0.1:{port_a}");
    let addr_c = format!("127.0.0.1:{port_c}");

    thread::spawn(move || run_node_c(port_c));
    thread::sleep(Duration::from_millis(50));
    thread::spawn(move || run_node_a(port_a));
    thread::sleep(Duration::from_millis(50));

    println!("\nstep 1: Node B registers routing rule on Node A");
    do_register(&addr_a, "44AWD", &addr_c, "88QWD");

    thread::sleep(Duration::from_millis(100));

    println!("\nstep 2: sender delivers packet for hash '44AWD' to Node A");
    do_send(&addr_a, "44AWD", "hello, hidden service!");

    thread::sleep(Duration::from_millis(200));

    println!("\nstep 3: packet for unknown hash is dropped by Node A");
    do_send(&addr_a, "UNKNOWN", "this should be silently dropped");

    thread::sleep(Duration::from_millis(200));

    println!("\nstep 4: a second rule for '44AWD' is refused (hijack attempt)");
    do_register(&addr_a, "44AWD", "127.0.0.1:9199", "88QWD");

    thread::sleep(Duration::from_millis(100));

    println!("\nstep 5: a rule pointing back at Node A is refused (routing loop)");
    do_register(&addr_a, "LOOPHASH", &addr_a, "LOOPHASH");

    thread::sleep(Duration::from_millis(200));
    println!("\ntest complete");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("node-a") => {
            let port = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(9110);
            run_node_a(port);
        }
        Some("node-c") => {
            let port = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(9111);
            run_node_c(port);
        }
        Some("register") => {
            let (aaddr, mhash, caddr, thash) = (
                args.get(2).expect("missing node-a addr"),
                args.get(3).expect("missing match-hash"),
                args.get(4).expect("missing node-c addr"),
                args.get(5).expect("missing target-hash"),
            );
            do_register(aaddr, mhash, caddr, thash);
        }
        Some("send") => {
            let (aaddr, dhash, msg) = (
                args.get(2).expect("missing node-a addr"),
                args.get(3).expect("missing dest-hash"),
                args.get(4).expect("missing message"),
            );
            do_send(aaddr, dhash, msg);
        }
        Some("test") | None => run_test(),
        Some(other) => {
            eprintln!("unknown command: {other}");
            eprintln!("usage:");
            eprintln!("  routing-rules node-a   <port>");
            eprintln!("  routing-rules node-c   <port>");
            eprintln!("  routing-rules register <node-a> <match-hash> <node-c> <target-hash>");
            eprintln!("  routing-rules send     <node-a> <dest-hash> <message>");
            eprintln!("  routing-rules test");
        }
    }
}
