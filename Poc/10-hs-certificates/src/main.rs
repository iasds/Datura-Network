mod address;
mod certificate;
mod circuit;
mod dlog;
mod equix_pow;
mod identity;
mod routing;
mod schnorr;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ff::PrimeField;
use group::ff::Field;
use pasta_curves::pallas;
use rand::rngs::OsRng;

use certificate::{
    Certificate, build_certificate, endorse_certificate, verify_certificate, verify_endorsement,
};
use circuit::hs_hash;
use dlog::derive_pk;
use equix_pow::{create_challenge, get_challenge_effort, solve_challenge, verify_solution};
use identity::{
    AddressBinding, IdentityKeys, canonical_hash, dn_address, generate_identity, publish_binding,
    resolve_hs_hash,
};
use routing::{RoutingInstruction, build_routing_instruction, verify_routing_instruction};

// Effort a one-day grant costs; longer grants are priced linearly from this
// (see required_effort). Doubles as the floor a standalone verifier enforces:
// no certificate can have paid less than one day's price.
const MIN_CHALLENGE_DIFFICULTY: u32 = 800;

// Longest grant Node A will sell. Even with duration-priced challenges the
// cap stays: Equi-X effort scales solve time linearly, and a years-long grant
// would need either an absurd single challenge or (better) periodic renewal,
// which is what the expiry mechanism is for.
const MAX_CERT_LIFETIME_SECS: u64 = 30 * 86_400;

// Tolerance for clock differences between the RDV node and any later verifier when checking a certificate's issued_at.
const CLOCK_SKEW_SECS: u64 = 300;

// Duration-priced PoW: the challenge difficulty scales with how long a grant
// Node B is asking to buy, at MIN_CHALLENGE_DIFFICULTY per (started) day.
// Node B now declares its desired expires inside the RDV request, before the challenge
// is issued, so Node A can price it.
fn required_effort(lifetime_secs: u64) -> u32 {
    let days = lifetime_secs.div_ceil(86_400).max(1);
    // Saturate in u64 before narrowing: casting days to u32 before the
    // multiply would wrap an astronomically long lifetime down toward zero
    // effort instead of saturating.
    u32::try_from(days.saturating_mul(MIN_CHALLENGE_DIFFICULTY as u64)).unwrap_or(u32::MAX)
}

// Much more than other poc's 30 secs: unlike simple packet forwarding, the
// gap between the challenge and the certificate message includes real PoW
// solving and real ZK proof generation (cold, uncached proving-key
// generation mostly), both of which can be over a minute on a single machine.
const IO_TIMEOUT: Duration = Duration::from_secs(400);

// How often Node A sweeps expired grants out of its routing table. Grants run
// for days, so the exact period only bounds how long a dead entry lingers;
// a minute seems to work, should be per node config
const REVOCATION_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

// Maximum accepted certificate size. A real certificate is a few KB (the
// halo2 proof dominates); this cap prevents a peer from triggering a ~4 GiB
// allocation by sending 0xFFFFFFFF as the 4-byte length prefix.
const MAX_CERT_LEN: usize = 1024 * 1024;

const MSG_REQUEST_RDV: u8 = 1;
const MSG_CHALLENGE: u8 = 2;
const MSG_CERTIFICATE: u8 = 3;
const MSG_ACK: u8 = 4;
const MSG_REJECT: u8 = 5;
const MSG_ROUTE: u8 = 6;

// What Node A remembers per granted hs_hash. The certificate is the public,
// shareable half; route_target is the private half from the routing
// instruction (option B in the issue thread) and must never leave this node.
struct RdvEntry {
    cert: Certificate,
    route_target: Option<[u8; 32]>,
    // Which handshake committed this grant. Sessions are numbered in accept
    // order, and a hidden service can have two in flight at once
    // (a renewal opened before the previous session finished, or a retry after a timeout).
    // Both would key the table on the same hs_hash, so without this
    // the second session's certificate could replace the first's between the
    // first's grant and its routing instruction, leaving the entry pairing one
    // session's certificate with the other's target.
    session: u64,
}

type RoutingTable = Arc<Mutex<HashMap<[u8; 32], RdvEntry>>>;

// Commits an accepted grant, replacing any grant already held for the same
// hidden service. Done under a single lock acquisition so a concurrent session
// cannot interleave between the read and the write.
// The new grant inherits the stored routing target: the hidden service just
// re-proved ownership of the same hash, and the target it installed earlier was
// authorized by that same key, so a renewal must not end a route
// that is already carrying traffic. (Node B is free to close the session
// without sending a fresh instruction.)
//
// Returns false without touching the table if a newer session already committed
// a grant for this hash, so a session that stalls in its proof/PoW phase can't
// displace a grant issued after it.
fn commit_grant(table: &RoutingTable, cert: Certificate, session: u64) -> bool {
    let hs = cert.hs_hash;
    let mut guard = table.lock().unwrap();
    let inherited = match guard.get(&hs) {
        Some(existing) if existing.session > session => return false,
        Some(existing) => existing.route_target,
        None => None,
    };
    guard.insert(
        hs,
        RdvEntry {
            cert,
            route_target: inherited,
            session,
        },
    );
    true
}

// Why a routing instruction could not be attached. Both reject, but they
// are different so they stay distinguishable rather than refusing
#[derive(Debug, PartialEq, Eq)]
enum AttachOutcome {
    // Attached; carries the grant's expiry.
    Attached(u64),
    // The entry is gone or belongs to a newer session.
    Superseded,
    // The grant this instruction belongs to expired before the instruction arrived.
    Expired,
}

// Attaches this session's routing target to its own grant.
//
// Superseded if the entry is gone or now belongs to a newer session. That
// target was authorized against the certificate this session got endorsed, so
// writing it onto a certificate from a different session would leave Node A
// routing a grant that never named its destination. Rejects rather than
// acknowledging, so Node B learns the instruction did not take and
// can resubmit it against its current grant.
//
// Expired if the window closed while this session was proving.
// Nothing prevents buying an unusably short grant because the proof takes longer than it's lifetime. 
// An expired grant is reachable for up to REVOCATION_SWEEP_INTERVAL and must not receive a route: the window is over, and the
// sweeper is about to drop it
fn attach_route(
    table: &RoutingTable,
    hs: [u8; 32],
    target: [u8; 32],
    session: u64,
    now: u64,
) -> AttachOutcome {
    let mut guard = table.lock().unwrap();
    match guard.get_mut(&hs) {
        Some(entry) if entry.session != session => AttachOutcome::Superseded,
        Some(entry) if entry.cert.expires <= now => AttachOutcome::Expired,
        Some(entry) => {
            entry.route_target = Some(target);
            AttachOutcome::Attached(entry.cert.expires)
        }
        None => AttachOutcome::Superseded,
    }
}

// Automatic revocation, node side. A certificate authorizes a window that was
// paid for up front, so the window just closes the grant after. There is no revocation message.
//
// Dropping the entry also drops route_target, the field this node is
// trusted to keep private, so an expired grant leaves no record.
//
// Returns how many grants were dropped.
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
    // Every node/HS hash in this system is a Poseidon output, i.e. a canonical
    // pallas base-field element. Reject anything else here at the untrusted-input
    // boundary, so a malformed hash can never reach the proving code (where it
    // would otherwise be a None the builders have to propagate).
    assert!(
        bool::from(pallas::Base::from_repr(arr).is_some()),
        "hash is not a canonical field element"
    );
    arr
}

// One framed JSON message: a type byte that must match `expected`, a 4-byte
// LE length (capped at MAX_CERT_LEN), then the JSON body. Returns None if the
// peer hung up or sent a different type; sends MSG_REJECT before returning
// None when the body is oversized or unparseable.
fn read_json_frame<T: serde::de::DeserializeOwned>(
    stream: &mut TcpStream,
    expected: u8,
) -> Option<T> {
    let mut type_buf = [0u8; 1];
    if !read_exact(stream, &mut type_buf) || type_buf[0] != expected {
        return None;
    }
    let mut len_buf = [0u8; 4];
    if !read_exact(stream, &mut len_buf) {
        return None;
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_CERT_LEN {
        let _ = stream.write_all(&[MSG_REJECT]);
        return None;
    }
    let mut body = vec![0u8; len];
    if !read_exact(stream, &mut body) {
        return None;
    }
    match serde_json::from_slice(&body) {
        Ok(v) => Some(v),
        Err(_) => {
            let _ = stream.write_all(&[MSG_REJECT]);
            None
        }
    }
}

// Node A: candidate rendezvous node. Issues PoW challenges, and grants RDV
// status to whoever submits a valid, freshly-solved certificate binding this
// node's own hash. Actual routing target (what Node A would route hash X's traffic to)
// is never part of this handshake or shared publicly. This PoC only covers the
// RDV-agreement step, not packet routing itself
fn run_node_a(port: u16) {
    run_node_a_with_identity(port, generate_identity());
}

// every node generates a default hidden service on startup, so it holds an
// Ed25519 key (.dn address) and a pallas key whose Poseidon hash is both
// its network identifier and its hashring position. Only the pallas half is
// used in the handshake below
fn run_node_a_with_identity(port: u16, id_a: IdentityKeys) {
    let sk_a = id_a.pallas_sk;
    let node_hash: [u8; 32] = canonical_hash(&id_a);
    println!("[node-a] address:       {}", dn_address(&id_a));
    println!("[node-a] identity hash: {}", hex::encode(node_hash));

    let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
    // Numbers each handshake in accept order, so concurrent sessions for the
    // same hidden service stay distinguishable.
    let sessions = AtomicU64::new(0);
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).expect("bind failed");
    println!("[node-a:{port}] listening");

    // Revoke expired grants on a timer rather than when entry is affected.
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

    for incoming in listener.incoming() {
        // A transient accept error (fd exhaustion under a connection flood, a
        // peer resetting between accept and return) must not take the whole
        // node down. Skip the bad connection and keep serving.
        let mut stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[node-a] accept error, skipping: {e}");
                continue;
            }
        };
        if stream.set_read_timeout(Some(IO_TIMEOUT)).is_err()
            || stream.set_write_timeout(Some(IO_TIMEOUT)).is_err()
        {
            continue;
        }
        let peer = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        let table = table.clone();
        let session = sessions.fetch_add(1, Ordering::Relaxed);
        thread::spawn(move || handle_node_a(&mut stream, &peer, sk_a, node_hash, table, session));
    }
}

fn handle_node_a(
    stream: &mut TcpStream,
    peer: &str,
    sk_a: pallas::Scalar,
    node_hash: [u8; 32],
    table: RoutingTable,
    session: u64,
) {
    let mut type_buf = [0u8; 1];
    if !read_exact(stream, &mut type_buf) {
        return;
    }
    if type_buf[0] != MSG_REQUEST_RDV {
        println!("[node-a] {peer}: unexpected message type {}", type_buf[0]);
        return;
    }

    // The request carries the desired expires so the challenge can be priced
    // from the requested lifetime before it's issued.
    let mut expires_buf = [0u8; 8];
    if !read_exact(stream, &mut expires_buf) {
        return;
    }
    let requested_expires = u64::from_le_bytes(expires_buf);
    let now = now_unix();
    if requested_expires <= now || requested_expires > now + MAX_CERT_LIFETIME_SECS {
        println!("[node-a] {peer}: requested expires {requested_expires} out of range, rejecting");
        let _ = stream.write_all(&[MSG_REJECT]);
        return;
    }

    let effort = required_effort(requested_expires - now);
    let challenge = create_challenge(effort);
    if stream.write_all(&[MSG_CHALLENGE]).is_err()
        || stream.write_all(&challenge.to_le_bytes()).is_err()
    {
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
        let _ = stream.write_all(&[MSG_REJECT]);
        return;
    }

    println!(
        "[node-a] {peer}: certificate accepted, agreed to route for hs_hash {} until {}",
        hex::encode(cert.hs_hash),
        cert.expires
    );
    // Counter-sign the accepted grant with this node's own identity key
    // and hand the endorsement back, so the completed certificate
    // carries public, standalone-verifiable proof that this node agreed
    let end = endorse_certificate(sk_a, &cert);
    let (rdv_pk, sig_r, sig_s) = (end.rdv_pk, end.sig_r, end.sig_s);
    cert.endorsement = Some(end);
    let hs = cert.hs_hash;
    if !commit_grant(&table, cert, session) {
        // A later session already granted this hidden service while this one
        // was still proving; its grant stands and this session must not touch
        // the entry (including in the routing phase below). Reject rather than
        // acknowledge, for the same reason as attach_route's None arm.
        println!("[node-a] {peer}: grant superseded by a newer session, not stored");
        let _ = stream.write_all(&[MSG_REJECT]);
        return;
    }

    let ok = stream.write_all(&[MSG_ACK]).is_ok()
        && stream.write_all(&rdv_pk).is_ok()
        && stream.write_all(&sig_r).is_ok()
        && stream.write_all(&sig_s).is_ok();
    if !ok {
        return;
    }

    // Optional second phase on the same session: the private routing instruction.
    // Node B, now holding confirmation that this node agreed to be its RDV, tells it where to
    // actually route. Signed by the same hidden key, verified with the same
    // circuit. If B closes the connection instead, the grant simply stands without a target yet.
    let Some(instr) = read_json_frame::<RoutingInstruction>(stream, MSG_ROUTE) else {
        return;
    };

    if !accept_instruction(&instr, hs, node_hash) {
        println!("[node-a] {peer}: routing instruction rejected");
        let _ = stream.write_all(&[MSG_REJECT]);
        return;
    }

    // The target stays in this node's table only; it is intentionally
    // never logged in full or shared anywhere.
    // The route only lives with the certificate that authorizes it.
    match attach_route(&table, hs, instr.target_node_hash, session, now_unix()) {
        AttachOutcome::Attached(expires) => {
            println!(
                "[node-a] {peer}: routing instruction accepted for hs_hash {} until {expires} (target kept private)",
                hex::encode(instr.hs_hash),
            );
            let _ = stream.write_all(&[MSG_ACK]);
        }
        // Acknowledging here would tell Node B its route is live while the
        // stored grant belongs to another session, so reject and let it retry.
        AttachOutcome::Superseded => {
            println!(
                "[node-a] {peer}: routing instruction for hs_hash {} dropped, grant superseded by a newer session",
                hex::encode(instr.hs_hash),
            );
            let _ = stream.write_all(&[MSG_REJECT]);
        }
        // The window Node B paid for closed while it was building the proof.
        // No retry: it has to buy a new grant.
        AttachOutcome::Expired => {
            println!(
                "[node-a] {peer}: routing instruction for hs_hash {} dropped, grant expired",
                hex::encode(instr.hs_hash),
            );
            let _ = stream.write_all(&[MSG_REJECT]);
        }
    }
}

// Node A's full acceptance check for a submitted certificate.
// verify_certificate alone only checks the ZK proof. Everything else here
// (PoW freshness/validity, hash match, expiry) is Node A's own
// logic, kept as a standalone function so it's directly testable without
// spinning up TCP connections.
// Ordered cheapest-first: the ZK proof verification is by far the most
// expensive step, so it must be gated behind the PoW/hash/expiry checks.
// Otherwise a peer that never solved any PoW could still burn this node's
// CPU on proof verification with garbage certificates.
fn accept_certificate(
    cert: &Certificate,
    node_hash: [u8; 32],
    issued_challenge: u128,
    requested_expires: u64,
) -> bool {
    if cert.rdv_node_hash != node_hash {
        return false;
    }
    // The challenge was priced for exactly the lifetime Node B requested;
    // a certificate claiming any other expires (longer OR different) wasn't
    // what was paid for.
    if cert.expires != requested_expires {
        return false;
    }
    let now = now_unix();
    if cert.expires <= now || cert.expires > now + MAX_CERT_LIFETIME_SECS {
        return false;
    }
    // issued_at must sit at the present (within skew). This node priced the
    // challenge for `requested_expires - now`; recording an issued_at far from
    // now would make the certificate's own window (expires - issued_at) disagree
    // with what was actually paid, and a later verifier recomputes the
    // price from that window. Pinning it here keeps the two consistent.
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

// Node A's acceptance check for the private routing instruction: it must
// name this node, must be for the hs_hash of the certificate endorsed this
// session (so an instruction can't attach to someone else's grant), and its ZK proof must verify.
fn accept_instruction(
    instr: &RoutingInstruction,
    granted_hs_hash: [u8; 32],
    node_hash: [u8; 32],
) -> bool {
    if instr.rdv_node_hash != node_hash || instr.hs_hash != granted_hs_hash {
        return false;
    }
    verify_routing_instruction(instr)
}

// Node B: the hidden service destination. Requests RDV status from a
// candidate node (declaring the desired expires so the challenge is priced
// for it), solves the PoW challenge, submits a certificate proving it owns
// the key behind hs_hash without revealing that key to Node A, and finally
// hands over the private routing instruction naming the actual target.
//
// Returns the certificate along with the hs's .dn address and the
// address binding a client needs. The address
// and binding are not sent to A.
fn request_rdv(
    node_a_addr: &str,
    node_a_hash_hex: &str,
    expires: u64,
    target_hash: Option<[u8; 32]>,
) -> (Certificate, String, AddressBinding) {
    let node_a_hash = parse_hex32(node_a_hash_hex);

    println!("[request-rdv] connecting to node-a at {node_a_addr}");
    let mut stream = TcpStream::connect(node_a_addr).expect("connect failed");
    stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();

    stream.write_all(&[MSG_REQUEST_RDV]).unwrap();
    stream.write_all(&expires.to_le_bytes()).unwrap();

    let mut resp_type = [0u8; 1];
    stream.read_exact(&mut resp_type).unwrap();
    assert_ne!(
        resp_type[0], MSG_REJECT,
        "node-a rejected the request (expires out of its accepted range?)"
    );
    assert_eq!(
        resp_type[0], MSG_CHALLENGE,
        "expected challenge from node-a"
    );
    let mut challenge_buf = [0u8; 16];
    stream.read_exact(&mut challenge_buf).unwrap();
    let challenge = u128::from_le_bytes(challenge_buf);
    println!(
        "[request-rdv] challenge received (difficulty {}), solving...",
        get_challenge_effort(challenge)
    );

    let threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let solution = solve_challenge(threads, challenge);
    println!("[request-rdv] solved, building certificate...");

    // The hidden service holds both keys. The .dn address
    // (Ed25519) is what a user is given and what 9's E2EE uses; the pallas
    // key is what the certificate proves ownership of. Node A is told
    // only the Poseidon hash of the pallas key.
    let id_b = generate_identity();
    let sk_b = id_b.pallas_sk;
    let hs_addr = dn_address(&id_b);
    let binding = publish_binding(&id_b);
    let hs_hash_hex = hex::encode(canonical_hash(&id_b));
    println!("[request-rdv] hidden service address: {hs_addr}");
    println!("[request-rdv] hidden service hash:    {hs_hash_hex}");

    // Record the issue time so a verifier can price the grant's full
    // window. node_a_hash was validated canonical in parse_hex32, so the build never returns None here.
    let issued_at = now_unix();
    let mut cert = build_certificate(sk_b, node_a_hash, issued_at, expires, challenge, solution)
        .expect("node-a hash validated canonical at parse time");

    let cert_json = serde_json::to_vec(&cert).unwrap();
    stream.write_all(&[MSG_CERTIFICATE]).unwrap();
    stream
        .write_all(&(cert_json.len() as u32).to_le_bytes())
        .unwrap();
    stream.write_all(&cert_json).unwrap();

    let mut resp = [0u8; 1];
    stream.read_exact(&mut resp).unwrap();
    match resp[0] {
        MSG_ACK => {
            // Node A follows its acknoledgement with an endorsement: its revealed
            // identity pk plus a Schnorr signature over the certificate's
            // public fields. Attach it, then independently verify it before
            // trusting it (Node A could send garbage bytes).
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
                println!("[request-rdv] certificate accepted, endorsement valid");
            } else {
                println!(
                    "[request-rdv] certificate accepted, but endorsement INVALID. discarding it"
                );
                cert.endorsement = None;
            }

            // With the RDV agreement confirmed, hand over the private routing
            // instruction: where traffic for this hs_hash should actually go
            // (Node C). Signed by the same sk as the certificate, so Node A
            // can check both artifacts name the same hidden principal. In the
            // demo, absent an explicit target, a random node hash stands in
            // for Node C.
            let target = target_hash
                .unwrap_or_else(|| hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr());
            println!(
                "[request-rdv] sending private routing instruction (target: {}...)",
                &hex::encode(target)[..16]
            );
            let instr = build_routing_instruction(sk_b, node_a_hash, target)
                .expect("hashes validated canonical");
            let instr_json = serde_json::to_vec(&instr).unwrap();
            stream.write_all(&[MSG_ROUTE]).unwrap();
            stream
                .write_all(&(instr_json.len() as u32).to_le_bytes())
                .unwrap();
            stream.write_all(&instr_json).unwrap();

            let mut route_resp = [0u8; 1];
            stream.read_exact(&mut route_resp).unwrap();
            match route_resp[0] {
                MSG_ACK => println!("[request-rdv] routing instruction accepted"),
                _ => println!("[request-rdv] routing instruction rejected"),
            }
        }
        MSG_REJECT => println!("[request-rdv] certificate rejected"),
        other => println!("[request-rdv] unexpected response byte {other}"),
    }

    (cert, hs_addr, binding)
}

// A certificate names the hidden service by its canonical hash, which is
// meaningless to a client with a .dn address. Resolve the address to its canonical hash through
// the cross-signatures, then compare. Shows the certificate's ZK proof
// was produced by the holder of the pallas key that this specific address
// cross-signed.
//
// Only a party that already knows the address can run this. Node A is never
// given the address or the binding. It can still ask for addresses it already
// knows and wants to censor (!)
fn check_address_binding(cert: &Certificate, dn_addr: &str, binding: &AddressBinding) -> bool {
    match resolve_hs_hash(dn_addr, binding) {
        Ok(resolved) if resolved == cert.hs_hash => {
            println!("address:        {dn_addr}");
            println!("binding:        valid, resolves to this certificate's hs_hash");
            true
        }
        Ok(resolved) => {
            println!("address:        {dn_addr}");
            println!(
                "binding:        MISMATCH, address resolves to {} not {}",
                hex::encode(resolved),
                hex::encode(cert.hs_hash)
            );
            false
        }
        Err(e) => {
            println!("address:        {dn_addr}");
            println!("binding:        INVALID ({e})");
            false
        }
    }
}

// Standalone, stateless verification: a PoC 10.1 requirement

// This checks everything a third party can check from the certificate
// alone: the ZK proof, the PoW solution against the embedded challenge,
// current-time expiry, and the RDV node's endorsement. Only a
// proof-valid answer comes from verify_certificate; the rest would be
// silently skipped if this printed "proof valid: true" alone.

// A standalone verifier must apply the PoW policy itself: the effort baked into
// pow_challenge is whatever the prover chose, so a colluding HS + RDV pair could
// try to mint a long-lived grant while paying for a short one. The certificate
// carries issued_at (bound into the RDV node's endorsement), so a third
// party can recompute the exact duration price: required_effort(expires - issued_at).
// Because issued_at must be in the past, a far-future expires forces
// a proportionally large window --> the pair cannot understate what they owe. We
// still keep the one-day floor for degenerate windows.

// `address_check` (a .dn address and the path to its binding) adds whether this certificate belongs to that
// address. It does not feed into any of the checks above (a
// certificate can be authentic and not be asked for)
fn verify_cert_file(path: &str, address_check: Option<(&str, &str)>) {
    let bytes = std::fs::read(path).expect("failed to read certificate file");
    let cert: Certificate = serde_json::from_slice(&bytes).expect("invalid certificate JSON");
    let now = now_unix();
    let effort = get_challenge_effort(cert.pow_challenge);
    // Price the certificate's own claimed window. issued_at must not be in the
    // future (a future issue time would shrink the priced window for free).
    let issued_ok = cert.issued_at <= now + CLOCK_SKEW_SECS;
    let priced_window = cert.expires.saturating_sub(cert.issued_at);
    let required = required_effort(priced_window).max(MIN_CHALLENGE_DIFFICULTY);
    let pow_ok =
        issued_ok && effort >= required && verify_solution(cert.pow_challenge, cert.pow_solution);
    let expired = cert.expires <= now;
    let proof_ok = verify_certificate(&cert);
    let endorsement_ok = verify_endorsement(&cert);

    println!("hs_hash:        {}", hex::encode(cert.hs_hash));
    println!("rdv_node_hash:  {}", hex::encode(cert.rdv_node_hash));
    println!("issued_at:      {}", cert.issued_at);
    println!(
        "expires:        {} ({})",
        cert.expires,
        if expired { "EXPIRED" } else { "not expired" }
    );
    println!("pow effort:     {effort} (required {required} for a {priced_window}s window)");
    println!("pow valid:      {pow_ok}");
    println!("proof valid:    {proof_ok}");
    println!(
        "endorsement:    {}",
        if cert.endorsement.is_some() {
            if endorsement_ok { "valid" } else { "INVALID" }
        } else {
            "absent"
        }
    );

    // Optional, and only possible if it knows the address:
    // confirm this certificate belongs to the hidden service asked
    // about, rather than some other hash that verifies.
    let binding_ok = address_check.map(|(dn_addr, binding_path)| {
        let raw: Vec<u8> = std::fs::read(binding_path).expect("failed to read binding file");
        let binding: AddressBinding = serde_json::from_slice(&raw).expect("invalid binding JSON");
        check_address_binding(&cert, dn_addr, &binding)
    });

    // An INVALID certificate means whoever
    // served it produced or relayed a forgery --> distrust
    // that node. A valid certificate that is not
    // this address's is somebody else's grant, and doesn't relate to
    // the node that sent it.
    let cert_ok = pow_ok && !expired && proof_ok && endorsement_ok;
    println!(
        "certificate:    {}",
        if cert_ok { "VALID" } else { "INVALID" }
    );
    if let Some(ok) = binding_ok {
        println!(
            "for address:    {}",
            match (ok, cert_ok) {
                (true, _) => "yes",
                (false, true) => "no (authentic certificate, but not this address's)",
                (false, false) => "no",
            }
        );
    }
}

fn run_test() {
    let port_a: u16 = 9120;
    let addr_a = format!("127.0.0.1:{port_a}");

    // In the self-contained demo we generate node-a's identity ourselves so we
    // can compute its identity hash directly, rather than needing an operator to
    // read it off node-a's log. Node A's own privacy guarantees are unaffected either way:
    // the hash is meant to be public, only the underlying keys are secret.
    let id_a = generate_identity();
    let node_a_hash = hex::encode(canonical_hash(&id_a));

    thread::spawn(move || run_node_a_with_identity(port_a, id_a));
    thread::sleep(Duration::from_millis(150));

    println!(
        "\nstep 1: Node B requests RDV status, submits a certificate, then the private routing instruction"
    );
    let (cert, hs_addr, binding) = request_rdv(&addr_a, &node_a_hash, now_unix() + 86_400, None);

    thread::sleep(Duration::from_millis(200));

    println!("\nstep 2: save the certificate and re-verify it completely standalone");
    let path = std::env::temp_dir().join("poc10_test_cert.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&cert).unwrap()).unwrap();
    verify_cert_file(path.to_str().unwrap(), None);

    // Node A only ever saw the hash. A client that was given the address can do
    // more: bind that address to the hash through the cross-signatures
    // and confirm the certificate is the one it was looking for.
    println!(
        "\nstep 3: a client that knows the .dn address checks the certificate belongs to them"
    );
    let binding_path = std::env::temp_dir().join("poc10_test_binding.json");
    std::fs::write(&binding_path, serde_json::to_vec_pretty(&binding).unwrap()).unwrap();
    verify_cert_file(
        path.to_str().unwrap(),
        Some((&hs_addr, binding_path.to_str().unwrap())),
    );

    // The same check under an unrelated address must fail, otherwise the
    // binding wouldn't prove which hidden service this is
    println!("\nstep 4: the same certificate checked against an unrelated address");
    let unrelated_address = dn_address(&generate_identity());
    verify_cert_file(
        path.to_str().unwrap(),
        Some((&unrelated_address, binding_path.to_str().unwrap())),
    );

    println!("\ntest complete");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("node-a") => {
            let port = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(9120);
            run_node_a(port);
        }
        Some("request-rdv") => {
            let addr = args.get(2).expect("missing node-a addr");
            let hash = args.get(3).expect("missing node-a hash (hex)");
            let expires = args
                .get(4)
                .map(|s| s.parse().expect("expires must be a unix timestamp"))
                .unwrap_or_else(|| now_unix() + 86_400);
            let target = args.get(5).map(|s| parse_hex32(s));
            let (cert, hs_addr, binding) = request_rdv(addr, hash, expires, target);

            // Write both halves out so `verify` can be run against them.
            // certificate is public; the binding goes to who
            // already have the address.
            let cert_path = std::env::temp_dir().join("poc10_cert.json");
            let binding_path = std::env::temp_dir().join("poc10_binding.json");
            std::fs::write(&cert_path, serde_json::to_vec_pretty(&cert).unwrap()).unwrap();
            std::fs::write(&binding_path, serde_json::to_vec_pretty(&binding).unwrap()).unwrap();
            println!(
                "[request-rdv] certificate written to {}",
                cert_path.display()
            );
            println!(
                "[request-rdv] address binding written to {}",
                binding_path.display()
            );
            println!(
                "[request-rdv] verify with: hs-certificates verify {} {hs_addr} {}",
                cert_path.display(),
                binding_path.display()
            );
        }
        Some("verify") => {
            let path = args.get(2).expect("missing certificate file path");
            // Both or neither: checking an address without its binding, or a
            // binding without the address it is supposed to belong to, is not
            // a check
            let address_check = match (args.get(3), args.get(4)) {
                (Some(addr), Some(binding)) => Some((addr.as_str(), binding.as_str())),
                (None, None) => None,
                _ => panic!(
                    "verify takes either no binding args, or both <dn-address> and <binding-file>"
                ),
            };
            verify_cert_file(path, address_check);
        }
        Some("test") | None => run_test(),
        Some(other) => {
            eprintln!("unknown command: {other}");
            eprintln!("usage:");
            eprintln!("  hs-certificates node-a       [port]");
            eprintln!(
                "  hs-certificates request-rdv  <node-a-addr> <node-a-hash-hex> [expires-unix-ts] [target-node-hash-hex]"
            );
            eprintln!("  hs-certificates verify       <cert-file> [dn-address] [binding-file]");
            eprintln!("  hs-certificates test");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A one-day grant, priced accordingly (one day = MIN_CHALLENGE_DIFFICULTY), issued now.
    fn setup() -> (pallas::Scalar, [u8; 32], u128, u64, u64) {
        let sk_a = pallas::Scalar::random(OsRng);
        let node_hash: [u8; 32] = hs_hash(derive_pk(sk_a)).to_repr();
        let issued_at = now_unix();
        let expires = issued_at + 86_400;
        let challenge = create_challenge(required_effort(86_400));
        (sk_a, node_hash, challenge, issued_at, expires)
    }

    #[test]
    fn effort_scales_linearly_with_requested_lifetime() {
        assert_eq!(required_effort(86_400), MIN_CHALLENGE_DIFFICULTY);
        // Partial days round up: 25h costs 2 days.
        assert_eq!(
            required_effort(86_400 + 3_600),
            2 * MIN_CHALLENGE_DIFFICULTY
        );
        assert_eq!(required_effort(30 * 86_400), 30 * MIN_CHALLENGE_DIFFICULTY);
        // Degenerate zero-length request still costs a day, never a free challenge.
        assert_eq!(required_effort(0), MIN_CHALLENGE_DIFFICULTY);
    }

    #[test]
    fn required_effort_saturates_instead_of_truncating() {
        // A lifetime whose day-count is a multiple of 2^32 must not wrap to a tiny effort; it saturates at u32::MAX.
        let huge = (1u64 << 32) * 86_400;
        assert_eq!(required_effort(huge), u32::MAX);
    }

    #[test]
    fn standalone_rejects_understated_duration_price() {
        // a grant claiming a long window while paying only one day's PoW.
        // With issued_at bound in, the standalone price check required_effort(expires - issued_at) is not met by a one-day
        // solution, so a verifier rejects it.
        let sk_b = pallas::Scalar::random(OsRng);
        let node_hash = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();
        let issued_at = now_unix();
        let expires = issued_at + 30 * 86_400; // claim a 30-day grant
        let challenge = create_challenge(required_effort(86_400)); // pay one day
        let solution = solve_challenge(1, challenge);
        let cert =
            build_certificate(sk_b, node_hash, issued_at, expires, challenge, solution).unwrap();

        let effort = get_challenge_effort(cert.pow_challenge);
        let priced_window = cert.expires.saturating_sub(cert.issued_at);
        let required = required_effort(priced_window).max(MIN_CHALLENGE_DIFFICULTY);
        assert!(
            effort < required,
            "a one-day payment must not satisfy a 30-day window's price"
        );
    }

    #[test]
    fn valid_certificate_accepted() {
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert =
            build_certificate(sk_b, node_hash, issued_at, expires, challenge, solution).unwrap();

        assert!(accept_certificate(&cert, node_hash, challenge, expires));
    }

    #[test]
    fn certificate_from_a_dual_keypair_identity_resolves_from_its_address() {
        // End to end: a certificate built with the pallas half
        // must be reachable from the Ed25519 half. This
        // makes a certificate usable, since the client starts from a .dn address
        // and certificate names a hash.
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let id = generate_identity();
        let cert = build_certificate(
            id.pallas_sk,
            node_hash,
            issued_at,
            expires,
            challenge,
            solution,
        )
        .unwrap();

        assert_eq!(cert.hs_hash, canonical_hash(&id));
        let binding = publish_binding(&id);
        assert!(check_address_binding(&cert, &dn_address(&id), &binding));

        // An unrelated address can't claim this certificate, or
        // the real address can't be paired with somebody else's binding.
        let random_identity = generate_identity();
        assert!(!check_address_binding(
            &cert,
            &dn_address(&random_identity),
            &binding
        ));
        assert!(!check_address_binding(
            &cert,
            &dn_address(&id),
            &publish_binding(&random_identity)
        ));
    }

    #[test]
    fn certificate_carries_no_trace_of_the_dn_address() {
        // The rendezvous node receives this. If the Ed25519
        // public key (the address) was in it, Node A
        // could recover the address and censor by name.
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let id = generate_identity();
        let cert = build_certificate(
            id.pallas_sk,
            node_hash,
            issued_at,
            expires,
            challenge,
            solution,
        )
        .unwrap();

        // serde renders every byte array in the certificate (hashes, signature
        // scalars, the halo2 proof) as a comma-separated list of decimals
        // so searching for the raw key bytes or their hex would match
        // nothing.
        let rendered = |bytes: &[u8]| {
            bytes
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let json = String::from_utf8(serde_json::to_vec(&cert).unwrap()).unwrap();
        let ed_pk = id.ed_sk.verifying_key();
        assert!(
            !json.contains(&rendered(ed_pk.as_bytes())),
            "certificate must not contain the Ed25519 public key"
        );

        // Check actual appearance
        assert!(
            json.contains(&rendered(&cert.hs_hash)),
            "certificate JSON should render its own hs_hash this way"
        );
    }

    #[test]
    fn replayed_pow_solution_from_different_challenge_rejected() {
        // A solution that solves some Equi-X challenge, but not
        // the one Node A actually issued this session, must not be reusable.
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let other_challenge = create_challenge(MIN_CHALLENGE_DIFFICULTY);
        let solution_for_other_challenge = solve_challenge(1, other_challenge);

        let sk_b = pallas::Scalar::random(OsRng);
        // Certificate honestly claims other_challenge, not the one Node A issued.
        let cert = build_certificate(
            sk_b,
            node_hash,
            issued_at,
            expires,
            other_challenge,
            solution_for_other_challenge,
        )
        .unwrap();

        assert!(!accept_certificate(&cert, node_hash, challenge, expires));
    }

    #[test]
    fn forged_pow_solution_rejected() {
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let sk_b = pallas::Scalar::random(OsRng);
        // Garbage solution bytes, never actually solved.
        let cert =
            build_certificate(sk_b, node_hash, issued_at, expires, challenge, [0u8; 24]).unwrap();

        assert!(!accept_certificate(&cert, node_hash, challenge, expires));
    }

    #[test]
    fn tampered_rdv_node_hash_rejected() {
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let mut cert =
            build_certificate(sk_b, node_hash, issued_at, expires, challenge, solution).unwrap();

        // A resolver or the RDV node itself tampering with which node the
        // certificate was actually made out to, after the fact.
        cert.rdv_node_hash = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();

        assert!(!accept_certificate(&cert, node_hash, challenge, expires));
    }

    #[test]
    fn expires_other_than_requested_rejected() {
        // The challenge was priced for the requested lifetime; a certificate
        // claiming a longer expires than was paid for must be rejected even
        // though everything else about it is valid.
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert = build_certificate(
            sk_b,
            node_hash,
            issued_at,
            expires + 7 * 86_400,
            challenge,
            solution,
        )
        .unwrap();

        assert!(!accept_certificate(&cert, node_hash, challenge, expires));
    }

    #[test]
    fn full_certificate_with_endorsement_verifies_standalone() {
        let (sk_a, node_hash, challenge, issued_at, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let mut cert =
            build_certificate(sk_b, node_hash, issued_at, expires, challenge, solution).unwrap();
        assert!(accept_certificate(&cert, node_hash, challenge, expires));
        cert.endorsement = Some(endorse_certificate(sk_a, &cert));

        // Standalone re-verification, what a third party holding
        // only the certificate can and should check. With issued_at bound in,
        // it recomputes the exact duration price rather than a bare floor.
        let effort = get_challenge_effort(cert.pow_challenge);
        let priced_window = cert.expires.saturating_sub(cert.issued_at);
        assert!(cert.issued_at <= now_unix() + CLOCK_SKEW_SECS);
        assert!(effort >= required_effort(priced_window));
        assert!(verify_solution(cert.pow_challenge, cert.pow_solution));
        assert!(cert.expires > now_unix());
        assert!(verify_certificate(&cert));
        assert!(verify_endorsement(&cert));

        // JSON round-trip preserves everything, endorsement included.
        let json = serde_json::to_vec(&cert).unwrap();
        let cert2: Certificate = serde_json::from_slice(&json).unwrap();
        assert!(verify_certificate(&cert2) && verify_endorsement(&cert2));
    }

    #[test]
    fn expired_certificate_rejected() {
        let (_, node_hash, challenge, _, _) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        // Already-expired timestamp, with the requested expires matching so
        // it's specifically the expiry check that rejects.
        let expired = now_unix() - 3600;
        let cert =
            build_certificate(sk_b, node_hash, now_unix(), expired, challenge, solution).unwrap();

        assert!(!accept_certificate(&cert, node_hash, challenge, expired));
    }

    #[test]
    fn certificate_beyond_max_lifetime_rejected() {
        let (_, node_hash, challenge, _, _) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        // Even if the request phase were bypassed, the acceptance check
        // itself refuses grants past the cap.
        let too_far = now_unix() + MAX_CERT_LIFETIME_SECS + 3600;
        let cert =
            build_certificate(sk_b, node_hash, now_unix(), too_far, challenge, solution).unwrap();

        assert!(!accept_certificate(&cert, node_hash, challenge, too_far));
    }

    #[test]
    fn valid_routing_instruction_accepted() {
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert =
            build_certificate(sk_b, node_hash, issued_at, expires, challenge, solution).unwrap();
        assert!(accept_certificate(&cert, node_hash, challenge, expires));

        // Same sk as the certificate: this is the honest flow.
        let target = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();
        let instr = build_routing_instruction(sk_b, node_hash, target).unwrap();
        assert!(accept_instruction(&instr, cert.hs_hash, node_hash));
    }

    #[test]
    fn routing_instruction_from_different_key_rejected() {
        // An attacker who saw a (public) certificate for hs_hash H tries to
        // attach its own routing instruction to that grant. Its instruction's
        // hs_hash can only ever be its OWN key's hash (the circuit binds
        // them), which won't match the granted H.
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert =
            build_certificate(sk_b, node_hash, issued_at, expires, challenge, solution).unwrap();

        let sk_attacker = pallas::Scalar::random(OsRng);
        let target = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();
        let instr = build_routing_instruction(sk_attacker, node_hash, target).unwrap();
        assert!(!accept_instruction(&instr, cert.hs_hash, node_hash));

        // And simply overwriting the claimed hs_hash breaks the proof.
        let mut forged = instr;
        forged.hs_hash = cert.hs_hash;
        assert!(!accept_instruction(&forged, cert.hs_hash, node_hash));
    }

    // These exercise commit_grant / attach_route directly: what matters is which session each write belongs
    // to, not the content, which accept_certificate and
    // accept_instruction have already cleared by the time either is called.

    // The session tests are about ordering, so they evaluate
    // every attach at an instant before dummy_cert's expiries
    const BEFORE_EXPIRY: u64 = 0;

    fn dummy_cert(hs: [u8; 32], expires: u64) -> Certificate {
        Certificate {
            hs_hash: hs,
            rdv_node_hash: [0u8; 32],
            issued_at: 0,
            expires,
            pow_challenge: 0,
            pow_solution: [0u8; 24],
            proof: Vec::new(),
            endorsement: None,
        }
    }

    #[test]
    fn concurrent_session_cannot_attach_route_to_another_sessions_grant() {
        // Two handshakes for one hidden service overlap (a renewal opened
        // before the first finished, or a retry after a timeout). Session 1 is
        // endorsed, session 2's certificate then supersedes it, and only then
        // does session 1 present its routing instruction. That target was
        // authorized against session 1's certificate, so pairing it with
        // session 2's would leave Node A routing under a grant that never
        // named that destination.
        let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
        let hs = [1u8; 32];

        assert!(commit_grant(&table, dummy_cert(hs, 1_000), 1));
        assert!(commit_grant(&table, dummy_cert(hs, 2_000), 2));

        // Refused, so Node B is told the instruction did not take.
        assert_eq!(
            attach_route(&table, hs, [0xAAu8; 32], 1, BEFORE_EXPIRY),
            AttachOutcome::Superseded
        );

        let guard = table.lock().unwrap();
        let entry = guard.get(&hs).unwrap();
        assert_eq!(entry.session, 2);
        assert_eq!(entry.cert.expires, 2_000);
        assert_eq!(entry.route_target, None);
    }

    #[test]
    fn renewal_keeps_the_route_already_installed() {
        // Node B may close after the grant without resending an instruction,
        // which is exactly what a renewal looks like. The route it installed
        // earlier was authorized by the same key that just re-proved ownership,
        // so it must survive; dropping it would stop live traffic.
        let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
        let hs = [2u8; 32];
        let target = [0xBBu8; 32];

        assert!(commit_grant(&table, dummy_cert(hs, 1_000), 1));
        assert_eq!(
            attach_route(&table, hs, target, 1, BEFORE_EXPIRY),
            AttachOutcome::Attached(1_000)
        );
        assert!(commit_grant(&table, dummy_cert(hs, 2_000), 2));

        let guard = table.lock().unwrap();
        let entry = guard.get(&hs).unwrap();
        assert_eq!(entry.cert.expires, 2_000, "renewed certificate is stored");
        assert_eq!(entry.route_target, Some(target), "route survives renewal");
    }

    #[test]
    fn stalled_older_session_cannot_displace_a_newer_grant() {
        // Sessions are numbered at accept time but finish out of order: a slow
        // PoW/proof phase can land session 1's grant after session 2's.
        let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
        let hs = [3u8; 32];
        let target = [0xCCu8; 32];

        assert!(commit_grant(&table, dummy_cert(hs, 2_000), 2));
        assert_eq!(
            attach_route(&table, hs, target, 2, BEFORE_EXPIRY),
            AttachOutcome::Attached(2_000)
        );

        assert!(!commit_grant(&table, dummy_cert(hs, 1_000), 1));
        assert_eq!(
            attach_route(&table, hs, [0xDDu8; 32], 1, BEFORE_EXPIRY),
            AttachOutcome::Superseded
        );

        let guard = table.lock().unwrap();
        let entry = guard.get(&hs).unwrap();
        assert_eq!(entry.session, 2);
        assert_eq!(entry.cert.expires, 2_000);
        assert_eq!(entry.route_target, Some(target));
    }

    #[test]
    fn grants_for_distinct_hidden_services_are_independent() {
        // Session guard is per-entry, so unrelated hidden services
        // handshaking concurrently never interfere with each other.
        let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
        let (hs1, hs2) = ([4u8; 32], [5u8; 32]);

        assert!(commit_grant(&table, dummy_cert(hs1, 1_000), 1));
        assert!(commit_grant(&table, dummy_cert(hs2, 1_000), 2));
        assert_eq!(
            attach_route(&table, hs1, [0xEEu8; 32], 1, BEFORE_EXPIRY),
            AttachOutcome::Attached(1_000)
        );
        assert_eq!(
            attach_route(&table, hs2, [0xFFu8; 32], 2, BEFORE_EXPIRY),
            AttachOutcome::Attached(1_000)
        );

        let guard = table.lock().unwrap();
        assert_eq!(guard.get(&hs1).unwrap().route_target, Some([0xEEu8; 32]));
        assert_eq!(guard.get(&hs2).unwrap().route_target, Some([0xFFu8; 32]));
    }

    #[test]
    fn sweep_revokes_expired_grants_and_keeps_live_ones() {
        // Automatic revocation
        let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
        let (expired, live) = ([6u8; 32], [7u8; 32]);

        assert!(commit_grant(&table, dummy_cert(expired, 1_000), 1));
        assert!(commit_grant(&table, dummy_cert(live, 3_000), 2));
        assert_eq!(
            attach_route(&table, expired, [0x11u8; 32], 1, BEFORE_EXPIRY),
            AttachOutcome::Attached(1_000)
        );

        assert_eq!(prune_expired(&table, 2_000), 1);

        let guard = table.lock().unwrap();
        // The route target goes with the grant. An expired grant can't leave a record of where its traffic was going
        assert!(guard.get(&expired).is_none());
        assert_eq!(guard.get(&live).unwrap().cert.expires, 3_000);
    }

    #[test]
    fn sweep_at_the_expiry_second_revokes() {
        // expires is the first second the grant is no longer valid, matching
        // accept_certificate and verify_cert_file with expires <= now as expired
        let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
        let hs = [8u8; 32];

        assert!(commit_grant(&table, dummy_cert(hs, 1_000), 1));
        assert_eq!(prune_expired(&table, 999), 0, "still inside the window");
        assert_eq!(prune_expired(&table, 1_000), 1);
    }

    #[test]
    fn expired_grant_takes_no_route() {
        // Node B can buy a grant lasting seconds, and building the routing
        // instruction's proof takes longer than that, so the instruction can
        // arrive after its own grant expired, before any sweep has run. It gets
        // rejected.
        let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
        let hs = [9u8; 32];

        assert!(commit_grant(&table, dummy_cert(hs, 1_000), 1));
        assert_eq!(
            attach_route(&table, hs, [0x22u8; 32], 1, 1_500),
            AttachOutcome::Expired
        );

        let guard = table.lock().unwrap();
        assert_eq!(guard.get(&hs).unwrap().route_target, None);
    }

    #[test]
    fn routing_instruction_for_wrong_rdv_node_rejected() {
        // An instruction made out to some other RDV node can't be submitted
        // to this one, even by the legitimate HS.
        let (_, node_hash, challenge, issued_at, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert =
            build_certificate(sk_b, node_hash, issued_at, expires, challenge, solution).unwrap();

        let other_node = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();
        let target = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();
        let instr = build_routing_instruction(sk_b, other_node, target).unwrap();
        assert!(!accept_instruction(&instr, cert.hs_hash, node_hash));
    }
}
