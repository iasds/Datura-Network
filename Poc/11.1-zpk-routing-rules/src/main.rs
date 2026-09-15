// PoC 11.1: ZK certificates gating routing rules
//
// PoC 11 installs a routing rule for anyone who pays a PoW.
// PoC 10 produces a certificate proving ownership
// of a hash without revealing the key behind it, plus a private routing
// instruction naming where that hash's traffic should go. This connects those.
#[allow(dead_code)]
mod address;
mod announce;
mod certificate;
mod circuit;
mod dlog;
mod equix_pow;
#[allow(dead_code)]
mod identity;
mod routing;
mod schnorr;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ff::PrimeField;
use group::ff::Field;
use pasta_curves::pallas;
use rand::rngs::OsRng;

use announce::{ANNOUNCE_LEN, Announce, build_announce, verify_announce};
use certificate::{
    Certificate, build_certificate, endorse_certificate, verify_certificate, verify_endorsement,
};
use circuit::hs_hash;
use dlog::derive_pk;
use equix_pow::{create_challenge, get_challenge_effort, solve_challenge, verify_solution};
use identity::{IdentityKeys, canonical_hash, dn_address, generate_identity};
use routing::{RoutingInstruction, build_routing_instruction, verify_routing_instruction};

// Wire format version, first byte of every message on every flow
const PROTO_VERSION: u8 = 1;

// from PoC 10 (certificate pricing / lifetime)

// Effort a one-day grant costs; longer grants are priced linearly from this.
// Doubles as the floor a standalone verifier enforces, and is what a node
// self-announcement costs.
const MIN_CHALLENGE_DIFFICULTY: u32 = 800;
const MAX_CERT_LIFETIME_SECS: u64 = 30 * 86_400;
const CLOCK_SKEW_SECS: u64 = 300;
const REVOCATION_SWEEP_INTERVAL: Duration = Duration::from_secs(60);
const MAX_CERT_LEN: usize = 1024 * 1024;

// PoC 11 (forwarding / resource limits)

const MAX_HOPS: u8 = 8;
const MAX_RULES: usize = 1024;
const MAX_CONNECTIONS: usize = 256;

// Entries in the hash -> address map. bounds how many distinct nodes can be held
const MAX_NODES: usize = 1024;

const IO_TIMEOUT: Duration = Duration::from_secs(30);
const REGISTER_IO_TIMEOUT: Duration = Duration::from_secs(400);

const MSG_ANNOUNCE: u8 = 1;
const MSG_REQUEST_RDV: u8 = 2;
const MSG_CHALLENGE: u8 = 3;
const MSG_CERTIFICATE: u8 = 4;
const MSG_ROUTE: u8 = 5;
const MSG_PACKET: u8 = 6;
const MSG_ACK: u8 = 7;
const MSG_REJECT: u8 = 8;

// Duration-priced PoW: the challenge difficulty scales with how long a grant
// Node B is asking to buy, at MIN_CHALLENGE_DIFFICULTY per (started) day.
fn required_effort(lifetime_secs: u64) -> u32 {
    let days = lifetime_secs.div_ceil(86_400).max(1);
    // Saturate in u64 before narrowing
    u32::try_from(days.saturating_mul(MIN_CHALLENGE_DIFFICULTY as u64)).unwrap_or(u32::MAX)
}

// What Node A remembers per granted hs_hash. The certificate is public.
// route_target and the address are private
struct RdvEntry {
    cert: Certificate,
    route_target: Option<[u8; 32]>,
    // Resolved from route_target through the node list at acceptance
    target_addr: Option<SocketAddr>,
    // Which handshake committed this grant
    session: u64,
}

type RoutingTable = Arc<Mutex<HashMap<[u8; 32], RdvEntry>>>;

// Node hash -> the address that node announced itself from
type NodeList = Arc<Mutex<HashMap<[u8; 32], SocketAddr>>>;

// Commits an accepted grant, replacing any grant already held for the same hs
//
// Returns false without touching the table if a newer session already committed
// a grant for this hash
fn commit_grant(table: &RoutingTable, cert: Certificate, session: u64) -> bool {
    let hs = cert.hs_hash;
    let mut guard = table.lock().unwrap();
    let inherited = match guard.get(&hs) {
        Some(existing) if existing.session > session => return false,
        Some(existing) => (existing.route_target, existing.target_addr),
        None => (None, None),
    };
    if guard.len() >= MAX_RULES && !guard.contains_key(&hs) {
        return false;
    }
    guard.insert(
        hs,
        RdvEntry {
            cert,
            route_target: inherited.0,
            target_addr: inherited.1,
            session,
        },
    );
    true
}

#[derive(Debug, PartialEq, Eq)]
enum AttachOutcome {
    // Attached; carries the grant's expiry.
    Attached(u64),
    // The entry is gone or belongs to a newer session
    Superseded,
    // The grant this instruction belongs to expired before the instruction arrived.
    Expired,
}

// Attaches this session's routing target to its own grant
fn attach_route(
    table: &RoutingTable,
    hs: [u8; 32],
    target: [u8; 32],
    addr: SocketAddr,
    session: u64,
    now: u64,
) -> AttachOutcome {
    let mut guard = table.lock().unwrap();
    match guard.get_mut(&hs) {
        Some(entry) if entry.session != session => AttachOutcome::Superseded,
        Some(entry) if entry.cert.expires <= now => AttachOutcome::Expired,
        Some(entry) => {
            entry.route_target = Some(target);
            entry.target_addr = Some(addr);
            AttachOutcome::Attached(entry.cert.expires)
        }
        None => AttachOutcome::Superseded,
    }
}

// Automatic revocation, node side
fn prune_expired(table: &RoutingTable, now: u64) -> usize {
    let mut guard = table.lock().unwrap();
    let before = guard.len();
    guard.retain(|_, entry| entry.cert.expires > now);
    before - guard.len()
}

fn read_exact(stream: &mut TcpStream, buf: &mut [u8]) -> bool {
    stream.read_exact(buf).is_ok()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn parse_hex32(s: &str) -> [u8; 32] {
    let bytes = hex::decode(s).expect("hash must be 64 hex chars");
    let arr: [u8; 32] = bytes.try_into().expect("hash must be exactly 32 bytes");
    // Every node/hs hash in this system is a Poseidon output. Reject anything else
    assert!(
        bool::from(pallas::Base::from_repr(arr).is_some()),
        "hash is not a canonical field element"
    );
    arr
}

fn reject(stream: &mut TcpStream) {
    let _ = write_msg(stream, MSG_REJECT);
}

fn write_msg(stream: &mut TcpStream, ty: u8) -> bool {
    stream.write_all(&[PROTO_VERSION, ty]).is_ok()
}

// Reads [version][type], returning the type. any version mismatch is refused
fn read_msg_header(stream: &mut TcpStream) -> Option<u8> {
    let mut buf = [0u8; 2];
    if !read_exact(stream, &mut buf) || buf[0] != PROTO_VERSION {
        return None;
    }
    Some(buf[1])
}

// One framed json message: [version][type][len: 4 LE][body], with length capped
fn read_json_frame<T: serde::de::DeserializeOwned>(
    stream: &mut TcpStream,
    expected: u8,
) -> Option<T> {
    if read_msg_header(stream)? != expected {
        return None;
    }
    let mut len_buf = [0u8; 4];
    if !read_exact(stream, &mut len_buf) {
        return None;
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_CERT_LEN {
        reject(stream);
        return None;
    }
    let mut body = vec![0u8; len];
    if !read_exact(stream, &mut body) {
        return None;
    }
    match serde_json::from_slice(&body) {
        Ok(v) => Some(v),
        Err(_) => {
            reject(stream);
            None
        }
    }
}

fn write_json_frame<T: serde::Serialize>(stream: &mut TcpStream, ty: u8, value: &T) -> bool {
    let body = match serde_json::to_vec(value) {
        Ok(b) => b,
        Err(_) => return false,
    };
    write_msg(stream, ty)
        && stream.write_all(&(body.len() as u32).to_le_bytes()).is_ok()
        && stream.write_all(&body).is_ok()
}

// Decrements live-connection count when a handler thread ends
struct ConnectionGuard(Arc<AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

// the shared accept loop
fn serve<F>(port: u16, label: &'static str, handler: F)
where
    F: Fn(&mut TcpStream, &str, u64) + Send + Sync + 'static,
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
    let sessions = AtomicU64::new(0);

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
        // Assigned here, not inside the thread
        let session = sessions.fetch_add(1, Ordering::Relaxed);
        let handler = handler.clone();
        let spawned = thread::Builder::new().spawn(move || {
            let _guard = guard;
            (*handler)(&mut stream, &peer, session);
        });
        if spawned.is_err() {
            println!("[{label}] could not spawn handler thread, dropping");
        }
    }
}

// Structural checks on an address this node might be asked to send to
fn validate_target(addr: SocketAddr, self_port: u16) -> Result<(), &'static str> {
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
    if addr.port() == self_port && ip.is_loopback() {
        return Err("target address is this node");
    }
    Ok(())
}

// Node A: the rendezvous / routing node
fn run_node_a(port: u16) {
    run_node_a_with_identity(port, generate_identity());
}

fn run_node_a_with_identity(port: u16, id_a: IdentityKeys) {
    let sk_a = id_a.pallas_sk;
    let node_hash: [u8; 32] = canonical_hash(&id_a);
    println!("[node-a] address:       {}", dn_address(&id_a));
    println!("[node-a] identity hash: {}", hex::encode(node_hash));

    let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
    let node_list: NodeList = Arc::new(Mutex::new(HashMap::new()));

    // Revoke expired grants on a timer
    {
        let table = table.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(REVOCATION_SWEEP_INTERVAL);
                let dropped = prune_expired(&table, now_unix());
                if dropped > 0 {
                    println!("[node-a] revoked {dropped} expired grant(s)");
                }
            }
        });
    }

    serve(port, "node-a", move |stream, peer, session| {
        let Some(ty) = read_msg_header(stream) else {
            return;
        };
        match ty {
            MSG_ANNOUNCE => handle_announce(stream, peer, &node_list, port),
            MSG_REQUEST_RDV => {
                handle_register(stream, peer, sk_a, node_hash, &table, &node_list, session)
            }
            MSG_PACKET => handle_packet(stream, peer, &table),
            other => println!("[node-a] {peer}: unknown message type {other}"),
        }
    });
}

// A node binds its canonical hash to a reachable address
fn handle_announce(stream: &mut TcpStream, peer: &str, node_list: &NodeList, self_port: u16) {
    let challenge = create_challenge(MIN_CHALLENGE_DIFFICULTY);
    if !write_msg(stream, MSG_CHALLENGE) || stream.write_all(&challenge.to_le_bytes()).is_err() {
        return;
    }

    let mut ver = [0u8; 1];
    if !read_exact(stream, &mut ver) || ver[0] != PROTO_VERSION {
        return;
    }
    let mut body = [0u8; ANNOUNCE_LEN];
    if !read_exact(stream, &mut body) {
        return;
    }
    let ann = Announce::from_bytes(&body);

    // cheapest first
    if !verify_solution(challenge, ann.pow_solution) {
        println!("[node-a] {peer}: announce with bad PoW, rejecting");
        reject(stream);
        return;
    }
    let Some(node_hash) = verify_announce(&ann, challenge) else {
        println!("[node-a] {peer}: announce signature invalid, rejecting");
        reject(stream);
        return;
    };

    // The ip is taken from the connection, not the message
    let Ok(src) = stream.peer_addr() else {
        return;
    };
    let addr = SocketAddr::new(src.ip(), ann.port);
    if let Err(why) = validate_target(addr, self_port) {
        println!("[node-a] {peer}: announce {why}, rejecting");
        reject(stream);
        return;
    }

    {
        let mut list = node_list.lock().unwrap();
        // cap only bounds max distinct nodes
        if !list.contains_key(&node_hash) && list.len() >= MAX_NODES {
            println!("[node-a] {peer}: node list full, rejecting");
            drop(list);
            reject(stream);
            return;
        }
        list.insert(node_hash, addr);
    }

    println!(
        "[node-a] {peer}: node {} announced at {addr}",
        &hex::encode(node_hash)[..16]
    );
    let _ = write_msg(stream, MSG_ACK);
}

// 10's RDV handshake, with the routing instruction's target resolved through this node's own list.
// 11's PoW-only registration is gone, the certificate's challenge is issued here, and priced by grant duration
#[allow(clippy::too_many_arguments)]
fn handle_register(
    stream: &mut TcpStream,
    peer: &str,
    sk_a: pallas::Scalar,
    node_hash: [u8; 32],
    table: &RoutingTable,
    node_list: &NodeList,
    session: u64,
) {
    // promote the connection
    if stream.set_read_timeout(Some(REGISTER_IO_TIMEOUT)).is_err()
        || stream.set_write_timeout(Some(REGISTER_IO_TIMEOUT)).is_err()
    {
        return;
    }

    let mut expires_buf = [0u8; 8];
    if !read_exact(stream, &mut expires_buf) {
        return;
    }
    let requested_expires = u64::from_le_bytes(expires_buf);
    let now = now_unix();
    if requested_expires <= now || requested_expires > now + MAX_CERT_LIFETIME_SECS {
        println!("[node-a] {peer}: requested expires {requested_expires} out of range, rejecting");
        reject(stream);
        return;
    }

    let effort = required_effort(requested_expires - now);
    let challenge = create_challenge(effort);
    if !write_msg(stream, MSG_CHALLENGE) || stream.write_all(&challenge.to_le_bytes()).is_err() {
        return;
    }
    println!(
        "[node-a] {peer}: RDV request until {requested_expires}, sent challenge (difficulty {effort})"
    );

    let Some(mut cert) = read_json_frame::<Certificate>(stream, MSG_CERTIFICATE) else {
        return;
    };

    if !accept_certificate(&cert, node_hash, challenge, requested_expires) {
        println!("[node-a] {peer}: certificate rejected");
        reject(stream);
        return;
    }

    println!(
        "[node-a] {peer}: certificate accepted, agreed to route for hs_hash {} until {}",
        hex::encode(cert.hs_hash),
        cert.expires
    );
    let end = endorse_certificate(sk_a, &cert);
    let (rdv_pk, sig_r, sig_s) = (end.rdv_pk, end.sig_r, end.sig_s);
    cert.endorsement = Some(end);
    let hs = cert.hs_hash;
    if !commit_grant(table, cert, session) {
        println!("[node-a] {peer}: grant superseded by a newer session, not stored");
        reject(stream);
        return;
    }

    let ok = write_msg(stream, MSG_ACK)
        && stream.write_all(&rdv_pk).is_ok()
        && stream.write_all(&sig_r).is_ok()
        && stream.write_all(&sig_s).is_ok();
    if !ok {
        return;
    }

    // Optional second phase: private routing instruction
    // If Node B closes, grant stands with no target
    let Some(instr) = read_json_frame::<RoutingInstruction>(stream, MSG_ROUTE) else {
        return;
    };

    let Some(addr) = accept_instruction(&instr, hs, node_hash, node_list) else {
        println!("[node-a] {peer}: routing instruction rejected");
        reject(stream);
        return;
    };

    match attach_route(table, hs, instr.target_node_hash, addr, session, now_unix()) {
        AttachOutcome::Attached(expires) => {
            println!(
                "[node-a] {peer}: routing instruction accepted for hs_hash {} until {expires}",
                hex::encode(instr.hs_hash),
            );
            let _ = write_msg(stream, MSG_ACK);
        }
        AttachOutcome::Superseded => {
            println!(
                "[node-a] {peer}: routing instruction for hs_hash {} dropped, grant superseded",
                hex::encode(instr.hs_hash),
            );
            reject(stream);
        }
        AttachOutcome::Expired => {
            println!(
                "[node-a] {peer}: routing instruction for hs_hash {} dropped, grant expired",
                hex::encode(instr.hs_hash),
            );
            reject(stream);
        }
    }
}

// Node A's full acceptance check for a submitted certificate. verify_certificate
// checks the ZK proof. PoW freshness, hash match and expiry are here
fn accept_certificate(
    cert: &Certificate,
    node_hash: [u8; 32],
    issued_challenge: u128,
    requested_expires: u64,
) -> bool {
    if cert.rdv_node_hash != node_hash {
        return false;
    }
    // The challenge was priced for the lifetime requested
    if cert.expires != requested_expires {
        return false;
    }
    let now = now_unix();
    if cert.expires <= now || cert.expires > now + MAX_CERT_LIFETIME_SECS {
        return false;
    }
    if cert.issued_at + CLOCK_SKEW_SECS < now || cert.issued_at > now + CLOCK_SKEW_SECS {
        return false;
    }
    if cert.pow_challenge != issued_challenge
        || !verify_solution(cert.pow_challenge, cert.pow_solution)
    {
        return false;
    }
    verify_certificate(cert)
}

// Node A's acceptance check for the private routing instruction
// - must name this node
// - must be for hs_hash endorsed this session
// - target must be a known node
fn accept_instruction(
    instr: &RoutingInstruction,
    granted_hs_hash: [u8; 32],
    node_hash: [u8; 32],
    node_list: &NodeList,
) -> Option<SocketAddr> {
    if instr.rdv_node_hash != node_hash || instr.hs_hash != granted_hs_hash {
        return None;
    }
    let addr = node_list
        .lock()
        .unwrap()
        .get(&instr.target_node_hash)
        .copied()?;
    if !verify_routing_instruction(instr) {
        return None;
    }
    Some(addr)
}

fn handle_packet(stream: &mut TcpStream, peer: &str, table: &RoutingTable) {
    // [hops_left: 1][dest_hash: 32][payload_len: 2 LE][payload: N]
    let mut hops_buf = [0u8; 1];
    if !read_exact(stream, &mut hops_buf) {
        return;
    }
    let hops_left = hops_buf[0];

    let mut dest_hash = [0u8; 32];
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
        "[node-a] {peer}: packet arrived for hash {} ({hops_left} hops left)",
        &hex::encode(dest_hash)[..16]
    );

    if hops_left == 0 {
        println!("[node-a] {peer}: hop limit reached, dropping");
        return;
    }

    // A grant with no instruction attached is valid
    let rule = {
        let t = table.lock().unwrap();
        t.get(&dest_hash)
            .filter(|e| e.cert.expires > now_unix())
            .and_then(|e| Some((e.target_addr?, e.route_target?)))
    };

    match rule {
        None => {
            // Unknown hash
            println!(
                "[node-a] {peer}: no live route for {}, dropping",
                &hex::encode(dest_hash)[..16]
            );
        }
        Some((target_addr, target_hash)) => {
            println!(
                "[node-a] {peer}: forwarding to {target_addr} re-addressed as {}",
                &hex::encode(target_hash)[..16]
            );
            match TcpStream::connect(target_addr) {
                Err(e) => println!("[node-a] connect to {target_addr} failed: {e}"),
                Ok(mut fwd) => {
                    let ok = fwd.set_write_timeout(Some(IO_TIMEOUT)).is_ok()
                        && write_msg(&mut fwd, MSG_PACKET)
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

// Node C: the destination
// announces to Node A so it is resolvable, then listens for forwarded packets.
fn run_node_c(node_a_addr: &str, port: u16) {
    run_node_c_with_identity(node_a_addr, port, generate_identity());
}

fn run_node_c_with_identity(node_a_addr: &str, port: u16, id_c: IdentityKeys) {
    let node_hash = canonical_hash(&id_c);
    println!("[node-c] address:       {}", dn_address(&id_c));
    println!("[node-c] identity hash: {}", hex::encode(node_hash));

    if do_announce(node_a_addr, &id_c, port) {
        println!("[node-c] announced to node-a at {node_a_addr}");
    } else {
        println!("[node-c] announcement to {node_a_addr} was refused");
    }

    serve(port, "node-c", move |stream, peer, _session| {
        let Some(ty) = read_msg_header(stream) else {
            return;
        };
        if ty != MSG_PACKET {
            println!("[node-c] {peer}: unexpected message type {ty}");
            return;
        }
        let mut hops = [0u8; 1];
        let mut dest_hash = [0u8; 32];
        let mut len_buf = [0u8; 2];
        if !read_exact(stream, &mut hops)
            || !read_exact(stream, &mut dest_hash)
            || !read_exact(stream, &mut len_buf)
        {
            return;
        }
        let mut payload = vec![0u8; u16::from_le_bytes(len_buf) as usize];
        if !read_exact(stream, &mut payload) {
            return;
        }
        println!("[node-c] packet received from {peer}:");
        println!("  dest_hash : {}", hex::encode(dest_hash));
        println!(
            "  for me    : {}",
            if dest_hash == node_hash { "yes" } else { "no" }
        );
        println!("  payload   : {:?}", String::from_utf8_lossy(&payload));
    });
}

// tells A this node's canonical hash, binding it to the port given.
// The address Node A records is this nodes source ip + port.
fn do_announce(node_a_addr: &str, id: &IdentityKeys, port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(node_a_addr) else {
        println!("[announce] could not connect to {node_a_addr}");
        return false;
    };
    if stream.set_read_timeout(Some(IO_TIMEOUT)).is_err()
        || stream.set_write_timeout(Some(IO_TIMEOUT)).is_err()
        || !write_msg(&mut stream, MSG_ANNOUNCE)
    {
        return false;
    }

    if read_msg_header(&mut stream) != Some(MSG_CHALLENGE) {
        return false;
    }
    let mut challenge_buf = [0u8; 16];
    if !read_exact(&mut stream, &mut challenge_buf) {
        return false;
    }
    let challenge = u128::from_le_bytes(challenge_buf);

    let threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let solution = solve_challenge(threads, challenge);
    let ann = build_announce(
        id.pallas_sk,
        port,
        challenge,
        solution,
        pallas::Scalar::random(OsRng),
    );

    if stream.write_all(&[PROTO_VERSION]).is_err() || stream.write_all(&ann.to_bytes()).is_err() {
        return false;
    }
    read_msg_header(&mut stream) == Some(MSG_ACK)
}

// Node B: the hs dest. Buys an RDV grant from Node A by
// proving ownership of its hash, then tells it where to route.
fn do_register(
    node_a_addr: &str,
    node_a_hash_hex: &str,
    expires: u64,
    target_hash: [u8; 32],
    id_b: Option<IdentityKeys>,
) -> Option<(Certificate, IdentityKeys)> {
    let node_a_hash = parse_hex32(node_a_hash_hex);

    println!("[register] connecting to node-a at {node_a_addr}");
    let mut stream = TcpStream::connect(node_a_addr).expect("connect failed");
    stream.set_read_timeout(Some(REGISTER_IO_TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(REGISTER_IO_TIMEOUT)).unwrap();

    if !write_msg(&mut stream, MSG_REQUEST_RDV) {
        return None;
    }
    stream.write_all(&expires.to_le_bytes()).unwrap();

    match read_msg_header(&mut stream) {
        Some(MSG_CHALLENGE) => {}
        Some(MSG_REJECT) => {
            println!("[register] node-a refused the request");
            return None;
        }
        _ => {
            println!("[register] unexpected response from node-a");
            return None;
        }
    }
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
    println!("[register] solved, building certificate...");

    // hs has both keys, .dn (Ed25519) for users, pallas key for ownership proof
    // a is told onlu poseidon hash of pallas key
    let id_b = id_b.unwrap_or_else(generate_identity);
    let sk_b = id_b.pallas_sk;
    println!("[register] hidden service address: {}", dn_address(&id_b));
    println!(
        "[register] hidden service hash:    {}",
        hex::encode(canonical_hash(&id_b))
    );

    let issued_at = now_unix();
    let mut cert = build_certificate(sk_b, node_a_hash, issued_at, expires, challenge, solution)
        .expect("node-a hash validated canonical at parse time");

    if !write_json_frame(&mut stream, MSG_CERTIFICATE, &cert) {
        return None;
    }

    match read_msg_header(&mut stream) {
        Some(MSG_ACK) => {
            // Node A follows its ack with endorsement: its identity pk & a Schnorr signature over the certificate's public fields
            // Verify independently
            let mut rdv_pk = [0u8; 32];
            let mut sig_r = [0u8; 32];
            let mut sig_s = [0u8; 32];
            stream.read_exact(&mut rdv_pk).unwrap();
            stream.read_exact(&mut sig_r).unwrap();
            stream.read_exact(&mut sig_s).unwrap();
            cert.endorsement = Some(certificate::Endorsement {
                rdv_pk,
                sig_r,
                sig_s,
            });
            if verify_endorsement(&cert) {
                println!("[register] certificate accepted, endorsement valid");
            } else {
                println!("[register] certificate accepted, but endorsement INVALID, discarding");
                cert.endorsement = None;
            }

            println!(
                "[register] sending private routing instruction (target: {}...)",
                &hex::encode(target_hash)[..16]
            );
            let instr = build_routing_instruction(sk_b, node_a_hash, target_hash)
                .expect("hashes validated canonical");
            if !write_json_frame(&mut stream, MSG_ROUTE, &instr) {
                return None;
            }
            match read_msg_header(&mut stream) {
                Some(MSG_ACK) => println!("[register] routing instruction accepted"),
                _ => println!("[register] routing instruction rejected"),
            }
        }
        Some(MSG_REJECT) => {
            println!("[register] certificate rejected");
            return None;
        }
        other => {
            println!("[register] unexpected response {other:?}");
            return None;
        }
    }

    Some((cert, id_b))
}

fn do_send(node_a_addr: &str, dest_hash_hex: &str, message: &str) {
    let dest_hash = parse_hex32(dest_hash_hex);
    let mut stream = TcpStream::connect(node_a_addr).expect("connect failed");
    stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
    println!(
        "[send] --> node-a at {node_a_addr}, hash {}, {} bytes",
        &dest_hash_hex[..16.min(dest_hash_hex.len())],
        message.len()
    );
    let _ = write_msg(&mut stream, MSG_PACKET)
        && stream.write_all(&[MAX_HOPS]).is_ok()
        && stream.write_all(&dest_hash).is_ok()
        && stream
            .write_all(&(message.len() as u16).to_le_bytes())
            .is_ok()
        && stream.write_all(message.as_bytes()).is_ok();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("node-a") => {
            let port = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(9130);
            run_node_a(port);
        }
        Some("node-c") => {
            let node_a_addr = args.get(2).expect("usage: node-c <node-a-addr> [port]");
            let port = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(9131);
            run_node_c(node_a_addr, port);
        }
        Some("register") => {
            let node_a_addr = args
                .get(2)
                .expect("usage: register <node-a-addr> <node-a-hash> <target-hash> [expires]");
            let node_a_hash = args.get(3).expect("missing node-a hash");
            let target_hash = parse_hex32(args.get(4).expect("missing target hash"));
            let expires = args
                .get(5)
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| now_unix() + 86_400);
            do_register(node_a_addr, node_a_hash, expires, target_hash, None);
        }
        Some("send") => {
            let node_a_addr = args
                .get(2)
                .expect("usage: send <node-a-addr> <hash> <message>");
            let dest_hash = args.get(3).expect("missing destination hash");
            let message = args.get(4).expect("missing message");
            do_send(node_a_addr, dest_hash, message);
        }
        Some("test") | None => run_test(),
        Some(other) => {
            println!("unknown command: {other}");
            println!("usage:");
            println!("  zpk-routing-rules node-a [port]");
            println!("  zpk-routing-rules node-c <node-a-addr> [port]");
            println!(
                "  zpk-routing-rules register <node-a-addr> <node-a-hash> <target-hash> [expires]"
            );
            println!("  zpk-routing-rules send <node-a-addr> <hash> <message>");
            println!("  zpk-routing-rules test");
        }
    }
}

fn run_test() {
    let port_a: u16 = 9130;
    let port_c: u16 = 9131;
    let addr_a = format!("127.0.0.1:{port_a}");

    // Generated so the demo can print node-a's hash
    let id_a = generate_identity();
    let node_a_hash = hex::encode(canonical_hash(&id_a));
    let id_c = generate_identity();
    let node_c_hash = canonical_hash(&id_c);

    thread::spawn(move || run_node_a_with_identity(port_a, id_a));
    thread::sleep(Duration::from_millis(150));

    println!("\nstep 1: Node C announces itself to Node A");
    {
        let addr_a = addr_a.clone();
        thread::spawn(move || run_node_c_with_identity(&addr_a, port_c, id_c));
    }
    thread::sleep(Duration::from_millis(400));

    println!("\nstep 2: Node B proves ownership of its hash and buys an RDV grant");
    let (cert, id_b) = do_register(
        &addr_a,
        &node_a_hash,
        now_unix() + 86_400,
        node_c_hash,
        None,
    )
    .expect("registration should succeed");
    let hs_hash_hex = hex::encode(cert.hs_hash);
    thread::sleep(Duration::from_millis(200));

    println!("\nstep 3: a packet for the hidden service's hash is forwarded to Node C");
    do_send(&addr_a, &hs_hash_hex, "hello, hidden service!");
    thread::sleep(Duration::from_millis(400));

    println!("\nstep 4: a packet for an unrouted hash is dropped");
    let stranger = hex::encode(hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr());
    do_send(&addr_a, &stranger, "nobody is listening for this");
    thread::sleep(Duration::from_millis(300));

    println!("\nstep 5: the same hidden service renews its grant");
    do_register(
        &addr_a,
        &node_a_hash,
        now_unix() + 86_400,
        node_c_hash,
        Some(id_b),
    )
    .expect("renewal by the owning key should succeed");
    thread::sleep(Duration::from_millis(200));

    println!("\nstep 6: a different hidden service cannot take over that hash");
    // A fresh identity registering names its own hs_hash
    let (other_cert, _) = do_register(
        &addr_a,
        &node_a_hash,
        now_unix() + 86_400,
        node_c_hash,
        None,
    )
    .expect("an unrelated hidden service can still buy its own grant");
    println!(
        "  its grant landed on its own hash {}..., not {}...",
        &hex::encode(other_cert.hs_hash)[..16],
        &hs_hash_hex[..16]
    );
    assert_ne!(other_cert.hs_hash, cert.hs_hash);

    println!("\nstep 7: the original route works");
    do_send(&addr_a, &hs_hash_hex, "still routed to the same place");
    thread::sleep(Duration::from_millis(400));

    println!("\ntest complete");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_hash(id: &IdentityKeys) -> [u8; 32] {
        canonical_hash(id)
    }

    fn fresh_cert(node_hash: [u8; 32], expires: u64) -> (Certificate, IdentityKeys) {
        let id = generate_identity();
        let cert =
            build_certificate(id.pallas_sk, node_hash, now_unix(), expires, 42, [7u8; 24]).unwrap();
        (cert, id)
    }

    fn table_with(cert: Certificate, session: u64) -> RoutingTable {
        let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
        assert!(commit_grant(&table, cert, session));
        table
    }

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port)
    }

    // required_effort / pricing

    #[test]
    fn effort_is_priced_per_started_day() {
        assert_eq!(required_effort(1), MIN_CHALLENGE_DIFFICULTY);
        assert_eq!(required_effort(86_400), MIN_CHALLENGE_DIFFICULTY);
        assert_eq!(required_effort(86_401), MIN_CHALLENGE_DIFFICULTY * 2);
        assert_eq!(required_effort(30 * 86_400), MIN_CHALLENGE_DIFFICULTY * 30);
    }

    #[test]
    fn absurd_lifetime_saturates_instead_of_wrapping() {
        assert_eq!(required_effort(u64::MAX), u32::MAX);
    }

    // accept_certificate

    #[test]
    fn certificate_for_another_node_rejected() {
        let node_hash = identity_hash(&generate_identity());
        let other = identity_hash(&generate_identity());
        let expires = now_unix() + 86_400;
        let (cert, _) = fresh_cert(other, expires);
        assert!(!accept_certificate(&cert, node_hash, 42, expires));
    }

    #[test]
    fn certificate_claiming_an_unpriced_expiry_rejected() {
        let node_hash = identity_hash(&generate_identity());
        let expires = now_unix() + 86_400;
        let (cert, _) = fresh_cert(node_hash, expires);
        // Priced for `expires`, certificate claims a longer window.
        assert!(!accept_certificate(&cert, node_hash, 42, expires + 86_400));
    }

    #[test]
    fn expired_certificate_rejected() {
        let node_hash = identity_hash(&generate_identity());
        let expires = now_unix() - 1;
        let (cert, _) = fresh_cert(node_hash, expires);
        assert!(!accept_certificate(&cert, node_hash, 42, expires));
    }

    #[test]
    fn certificate_with_stale_issued_at_rejected() {
        let node_hash = identity_hash(&generate_identity());
        let expires = now_unix() + 86_400;
        let (mut cert, _) = fresh_cert(node_hash, expires);
        cert.issued_at = now_unix() - (CLOCK_SKEW_SECS + 60);
        assert!(!accept_certificate(&cert, node_hash, 42, expires));
    }

    #[test]
    fn certificate_for_a_different_challenge_rejected() {
        let node_hash = identity_hash(&generate_identity());
        let expires = now_unix() + 86_400;
        let (cert, _) = fresh_cert(node_hash, expires);
        // Built against challenge 42, presented against a challenge this node
        // did not issue on this connection.
        assert!(!accept_certificate(&cert, node_hash, 43, expires));
    }

    // accept_instruction

    #[test]
    fn instruction_for_an_unknown_target_rejected() {
        let id_a = generate_identity();
        let node_hash = identity_hash(&id_a);
        let id_b = generate_identity();
        let target = identity_hash(&generate_identity());
        let instr = build_routing_instruction(id_b.pallas_sk, node_hash, target).unwrap();
        let node_list: NodeList = Arc::new(Mutex::new(HashMap::new()));
        assert!(accept_instruction(&instr, instr.hs_hash, node_hash, &node_list).is_none());
    }

    #[test]
    fn instruction_for_a_known_target_resolves_to_its_address() {
        let id_a = generate_identity();
        let node_hash = identity_hash(&id_a);
        let id_b = generate_identity();
        let target = identity_hash(&generate_identity());
        let instr = build_routing_instruction(id_b.pallas_sk, node_hash, target).unwrap();
        let node_list: NodeList = Arc::new(Mutex::new(HashMap::new()));
        node_list.lock().unwrap().insert(target, addr(9131));
        assert_eq!(
            accept_instruction(&instr, instr.hs_hash, node_hash, &node_list),
            Some(addr(9131))
        );
    }

    #[test]
    fn instruction_naming_another_rdv_node_rejected() {
        let id_a = generate_identity();
        let node_hash = identity_hash(&id_a);
        let elsewhere = identity_hash(&generate_identity());
        let id_b = generate_identity();
        let target = identity_hash(&generate_identity());
        let instr = build_routing_instruction(id_b.pallas_sk, elsewhere, target).unwrap();
        let node_list: NodeList = Arc::new(Mutex::new(HashMap::new()));
        node_list.lock().unwrap().insert(target, addr(9131));
        assert!(accept_instruction(&instr, instr.hs_hash, node_hash, &node_list).is_none());
    }

    #[test]
    fn instruction_for_a_grant_from_another_hidden_service_rejected() {
        // instruction can only attach to the grant endorsed in its own session
        let id_a = generate_identity();
        let node_hash = identity_hash(&id_a);
        let id_b = generate_identity();
        let target = identity_hash(&generate_identity());
        let instr = build_routing_instruction(id_b.pallas_sk, node_hash, target).unwrap();
        let node_list: NodeList = Arc::new(Mutex::new(HashMap::new()));
        node_list.lock().unwrap().insert(target, addr(9131));
        let someone_else = identity_hash(&generate_identity());
        assert!(accept_instruction(&instr, someone_else, node_hash, &node_list).is_none());
    }

    // commit_grant  / attach_route / prune_expired

    #[test]
    fn renewal_replaces_the_grant_and_inherits_the_route() {
        let node_hash = identity_hash(&generate_identity());
        let expires = now_unix() + 86_400;
        let (cert, id_b) = fresh_cert(node_hash, expires);
        let hs = cert.hs_hash;
        let table = table_with(cert, 0);

        let target = identity_hash(&generate_identity());
        assert_eq!(
            attach_route(&table, hs, target, addr(9131), 0, now_unix()),
            AttachOutcome::Attached(expires)
        );

        let renewal = build_certificate(
            id_b.pallas_sk,
            node_hash,
            now_unix(),
            expires + 3600,
            43,
            [8u8; 24],
        )
        .unwrap();
        assert_eq!(renewal.hs_hash, hs);
        assert!(commit_grant(&table, renewal, 1));

        let guard = table.lock().unwrap();
        let entry = guard.get(&hs).unwrap();
        assert_eq!(entry.route_target, Some(target));
        assert_eq!(entry.target_addr, Some(addr(9131)));
        assert_eq!(entry.cert.expires, expires + 3600);
    }

    #[test]
    fn a_stalled_older_session_cannot_displace_a_newer_grant() {
        let node_hash = identity_hash(&generate_identity());
        let expires = now_unix() + 86_400;
        let (cert, id_b) = fresh_cert(node_hash, expires);
        let hs = cert.hs_hash;
        let table = table_with(cert, 5);

        let stale = build_certificate(
            id_b.pallas_sk,
            node_hash,
            now_unix(),
            expires,
            44,
            [9u8; 24],
        )
        .unwrap();
        assert!(!commit_grant(&table, stale, 4));
        assert_eq!(table.lock().unwrap().get(&hs).unwrap().session, 5);
    }

    #[test]
    fn a_route_cannot_be_attached_to_another_sessions_grant() {
        let node_hash = identity_hash(&generate_identity());
        let expires = now_unix() + 86_400;
        let (cert, _) = fresh_cert(node_hash, expires);
        let hs = cert.hs_hash;
        let table = table_with(cert, 7);
        let target = identity_hash(&generate_identity());
        assert_eq!(
            attach_route(&table, hs, target, addr(9131), 6, now_unix()),
            AttachOutcome::Superseded
        );
    }

    #[test]
    fn a_route_cannot_be_attached_to_an_expired_grant() {
        let node_hash = identity_hash(&generate_identity());
        let now = now_unix();
        let (cert, _) = fresh_cert(node_hash, now + 10);
        let hs = cert.hs_hash;
        let table = table_with(cert, 0);
        let target = identity_hash(&generate_identity());
        assert_eq!(
            attach_route(&table, hs, target, addr(9131), 0, now + 11),
            AttachOutcome::Expired
        );
    }

    #[test]
    fn expired_grants_are_swept_and_take_their_route_with_them() {
        let node_hash = identity_hash(&generate_identity());
        let now = now_unix();
        let (cert, _) = fresh_cert(node_hash, now + 10);
        let hs = cert.hs_hash;
        let table = table_with(cert, 0);
        let target = identity_hash(&generate_identity());
        attach_route(&table, hs, target, addr(9131), 0, now);

        assert_eq!(prune_expired(&table, now), 0);
        assert_eq!(prune_expired(&table, now + 11), 1);
        assert!(table.lock().unwrap().is_empty());
    }

    // validate_target

    #[test]
    fn structurally_invalid_targets_rejected() {
        assert!(validate_target(addr(0), 9130).is_err());
        assert!(
            validate_target(
                SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 9131),
                9130
            )
            .is_err()
        );
        assert!(
            validate_target(
                SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::BROADCAST), 9131),
                9130
            )
            .is_err()
        );
        assert!(
            validate_target(
                SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::new(224, 0, 0, 1)), 9131),
                9130
            )
            .is_err()
        );
        // a node announcing this node's own listener would be a routing loop
        assert!(validate_target(addr(9130), 9130).is_err());
        assert!(validate_target(addr(9131), 9130).is_ok());
    }
}
