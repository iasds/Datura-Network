mod certificate;
mod circuit;
mod dlog;
mod equix_pow;
mod routing;
mod schnorr;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ff::PrimeField;
use group::ff::Field;
use pasta_curves::pallas;
use rand::rngs::OsRng;

use certificate::{
    build_certificate, endorse_certificate, verify_certificate, verify_endorsement, Certificate,
};
use circuit::hs_hash;
use dlog::derive_pk;
use equix_pow::{create_challenge, get_challenge_effort, solve_challenge, verify_solution};
use routing::{build_routing_instruction, verify_routing_instruction, RoutingInstruction};

// Effort a one-day grant costs; longer grants are priced linearly from this
// (see required_effort). Doubles as the floor a standalone verifier enforces:
// no certificate can have paid less than one day's price.
const MIN_CHALLENGE_DIFFICULTY: u32 = 800;

// Longest grant Node A will sell. Even with duration-priced challenges the
// cap stays: Equi-X effort scales solve time linearly, and a years-long grant
// would need either an absurd single challenge or (better) periodic renewal,
// which is what the expiry mechanism is for.
const MAX_CERT_LIFETIME_SECS: u64 = 30 * 86_400;

// Duration-priced PoW: the challenge difficulty scales with how long a grant
// Node B is asking to buy, at MIN_CHALLENGE_DIFFICULTY per (started) day.
// Node B now declares its desired expires inside the RDV request, before the challenge
// is issued, so Node A can price it.
fn required_effort(lifetime_secs: u64) -> u32 {
    let days = lifetime_secs.div_ceil(86_400).max(1);
    (days as u32).saturating_mul(MIN_CHALLENGE_DIFFICULTY)
}

// Much more than other poc's 30 secs: unlike simple packet forwarding, the
// gap between the challenge and the certificate message includes real PoW
// solving and real ZK proof generation (cold, uncached proving-key
// generation mostly), both of which can be over a minute on a single machine.
const IO_TIMEOUT: Duration = Duration::from_secs(400);

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
}

type RoutingTable = Arc<Mutex<HashMap<[u8; 32], RdvEntry>>>;

fn read_exact(stream: &mut TcpStream, buf: &mut [u8]) -> bool {
    stream.read_exact(buf).is_ok()
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn hex32(h: &[u8; 32]) -> String {
    hex::encode(h)
}

fn parse_hex32(s: &str) -> [u8; 32] {
    let bytes = hex::decode(s).expect("hash must be 64 hex chars");
    bytes.try_into().expect("hash must be exactly 32 bytes")
}

// Node A: candidate rendezvous node. Issues PoW challenges, and grants RDV
// status to whoever submits a valid, freshly-solved certificate binding this
// node's own hash. Actual routing target (what Node A would route hash X's traffic to) 
// is never part of this handshake or shared publicly. This PoC only covers the
// RDV-agreement step, not packet routing itself
fn run_node_a(port: u16) {
    run_node_a_with_key(port, pallas::Scalar::random(OsRng));
}

fn run_node_a_with_key(port: u16, sk_a: pallas::Scalar) {
    let node_hash: [u8; 32] = hs_hash(derive_pk(sk_a)).to_repr();
    println!("[node-a] identity hash: {}", hex32(&node_hash));

    let table: RoutingTable = Arc::new(Mutex::new(HashMap::new()));
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).expect("bind failed");
    println!("[node-a:{port}] listening");

    for incoming in listener.incoming() {
        let mut stream = incoming.unwrap();
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        let table = table.clone();
        thread::spawn(move || handle_node_a(&mut stream, &peer, sk_a, node_hash, table));
    }
}

fn handle_node_a(
    stream: &mut TcpStream,
    peer: &str,
    sk_a: pallas::Scalar,
    node_hash: [u8; 32],
    table: RoutingTable,
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
    if stream.write_all(&[MSG_CHALLENGE]).is_err() || stream.write_all(&challenge.to_le_bytes()).is_err() {
        return;
    }
    println!(
        "[node-a] {peer}: RDV request until {requested_expires}, sent challenge (difficulty {effort})"
    );

    let mut cert_type = [0u8; 1];
    if !read_exact(stream, &mut cert_type) || cert_type[0] != MSG_CERTIFICATE {
        return;
    }
    let mut len_buf = [0u8; 4];
    if !read_exact(stream, &mut len_buf) {
        return;
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_CERT_LEN {
        println!("[node-a] {peer}: certificate length {len} exceeds cap, rejecting");
        let _ = stream.write_all(&[MSG_REJECT]);
        return;
    }
    let mut cert_bytes = vec![0u8; len];
    if !read_exact(stream, &mut cert_bytes) {
        return;
    }
    let mut cert: Certificate = match serde_json::from_slice(&cert_bytes) {
        Ok(c) => c,
        Err(_) => {
            let _ = stream.write_all(&[MSG_REJECT]);
            return;
        }
    };

    if !accept_certificate(&cert, node_hash, challenge, requested_expires) {
        println!("[node-a] {peer}: certificate rejected");
        let _ = stream.write_all(&[MSG_REJECT]);
        return;
    }

    println!(
        "[node-a] {peer}: certificate accepted, agreed to route for hs_hash {} until {}",
        hex32(&cert.hs_hash),
        cert.expires
    );
    // Counter-sign the accepted grant with this node's own identity key
    // and hand the endorsement back, so the completed certificate
    // carries public, standalone-verifiable proof that this node agreed
    let end = endorse_certificate(sk_a, &cert);
    let ok = stream.write_all(&[MSG_ACK]).is_ok()
        && stream.write_all(&end.rdv_pk).is_ok()
        && stream.write_all(&end.sig_r).is_ok()
        && stream.write_all(&end.sig_s).is_ok();
    if !ok {
        return;
    }
    cert.endorsement = Some(end);
    let hs = cert.hs_hash;
    table.lock().unwrap().insert(hs, RdvEntry { cert, route_target: None });

    // Optional second phase on the same session: the private routing instruction.
    // Node B, now holding confirmation that this node agreed to be its RDV, tells it where to
    // actually route. Signed by the same hidden key, verified with the same
    // circuit. If B closes the connection instead, the grant simply stands without a target yet.
    let mut route_type = [0u8; 1];
    if !read_exact(stream, &mut route_type) || route_type[0] != MSG_ROUTE {
        return;
    }
    let mut len_buf = [0u8; 4];
    if !read_exact(stream, &mut len_buf) {
        return;
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_CERT_LEN {
        let _ = stream.write_all(&[MSG_REJECT]);
        return;
    }
    let mut instr_bytes = vec![0u8; len];
    if !read_exact(stream, &mut instr_bytes) {
        return;
    }
    let instr: RoutingInstruction = match serde_json::from_slice(&instr_bytes) {
        Ok(i) => i,
        Err(_) => {
            let _ = stream.write_all(&[MSG_REJECT]);
            return;
        }
    };

    if accept_instruction(&instr, hs, node_hash) {
        // The target stays in this node's table only; it is intentionally
        // never logged in full or shared anywhere.
        // The route lives and dies with the certificate that authorizes it.
        if let Some(entry) = table.lock().unwrap().get_mut(&hs) {
            entry.route_target = Some(instr.target_node_hash);
            println!(
                "[node-a] {peer}: routing instruction accepted for hs_hash {} until {} (target kept private)",
                hex32(&instr.hs_hash),
                entry.cert.expires
            );
        }
        let _ = stream.write_all(&[MSG_ACK]);
    } else {
        println!("[node-a] {peer}: routing instruction rejected");
        let _ = stream.write_all(&[MSG_REJECT]);
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
    if cert.pow_challenge != issued_challenge
        || !verify_solution(
            get_challenge_effort(issued_challenge),
            cert.pow_challenge,
            cert.pow_solution,
        )
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
fn request_rdv(
    node_a_addr: &str,
    node_a_hash_hex: &str,
    expires: u64,
    target_hash: Option<[u8; 32]>,
) -> Certificate {
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
    assert_eq!(resp_type[0], MSG_CHALLENGE, "expected challenge from node-a");
    let mut challenge_buf = [0u8; 16];
    stream.read_exact(&mut challenge_buf).unwrap();
    let challenge = u128::from_le_bytes(challenge_buf);
    println!(
        "[request-rdv] challenge received (difficulty {}), solving...",
        get_challenge_effort(challenge)
    );

    let threads = thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let solution = solve_challenge(threads, challenge);
    println!("[request-rdv] solved, building certificate...");

    let sk_b = pallas::Scalar::random(OsRng);
    let hs_hash_hex = hex32(&hs_hash(derive_pk(sk_b)).to_repr());
    println!("[request-rdv] hidden service hash: {hs_hash_hex}");

    let mut cert = build_certificate(sk_b, node_a_hash, expires, challenge, solution);

    let cert_json = serde_json::to_vec(&cert).unwrap();
    stream.write_all(&[MSG_CERTIFICATE]).unwrap();
    stream.write_all(&(cert_json.len() as u32).to_le_bytes()).unwrap();
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
            cert.endorsement = Some(certificate::Endorsement { rdv_pk, sig_r, sig_s });
            if verify_endorsement(&cert) {
                println!("[request-rdv] certificate accepted, endorsement valid");
            } else {
                println!("[request-rdv] certificate accepted, but endorsement INVALID. discarding it");
                cert.endorsement = None;
            }

            // With the RDV agreement confirmed, hand over the private routing
            // instruction: where traffic for this hs_hash should actually go
            // (Node C). Signed by the same sk as the certificate, so Node A
            // can check both artifacts name the same hidden principal. In the
            // demo, absent an explicit target, a random node hash stands in
            // for Node C.
            let target = target_hash.unwrap_or_else(|| {
                hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr()
            });
            println!("[request-rdv] sending private routing instruction (target: {}...)", &hex32(&target)[..16]);
            let instr = build_routing_instruction(sk_b, node_a_hash, target);
            let instr_json = serde_json::to_vec(&instr).unwrap();
            stream.write_all(&[MSG_ROUTE]).unwrap();
            stream.write_all(&(instr_json.len() as u32).to_le_bytes()).unwrap();
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

    cert
}

// Standalone, stateless verification: a PoC 10.1 requirement

// This checks everything a third party can check from the certificate
// alone: the ZK proof, the PoW solution against the embedded challenge, 
// current-time expiry, and the RDV node's endorsement. Only a 
// proof-valid answer comes from verify_certificate; the rest would be
// silently skipped if this printed "proof valid: true" alone.

// A standalone verifier must apply itself: the PoW difficulty is
// whatever effort value is embedded in pow_challenge. A
// colluding HS + RDV pair could mint a certificate with a trivial effort,
// so a verifier that cares about the payment being real must also enforce
// a minimum acceptable effort (a network-wide constant: at least
// MIN_CHALLENGE_DIFFICULTY, one day's price. A third party can't recompute
// the exact duration price without knowing the issue time).
fn verify_cert_file(path: &str) {
    let bytes = std::fs::read(path).expect("failed to read certificate file");
    let cert: Certificate = serde_json::from_slice(&bytes).expect("invalid certificate JSON");
    let effort = get_challenge_effort(cert.pow_challenge);
    let pow_ok = effort >= MIN_CHALLENGE_DIFFICULTY
        && verify_solution(effort, cert.pow_challenge, cert.pow_solution);
    let expired = cert.expires <= now_unix();
    let proof_ok = verify_certificate(&cert);
    let endorsement_ok = verify_endorsement(&cert);

    println!("hs_hash:        {}", hex32(&cert.hs_hash));
    println!("rdv_node_hash:  {}", hex32(&cert.rdv_node_hash));
    println!("expires:        {} ({})", cert.expires, if expired { "EXPIRED" } else { "not expired" });
    println!("pow effort:     {effort}");
    println!("pow valid:      {pow_ok}");
    println!("proof valid:    {proof_ok}");
    println!("endorsement:    {}", if cert.endorsement.is_some() {
        if endorsement_ok { "valid" } else { "INVALID" }
    } else {
        "absent"
    });
    println!(
        "certificate:    {}",
        if pow_ok && !expired && proof_ok && endorsement_ok { "VALID" } else { "INVALID" }
    );
}

fn run_test() {
    let port_a: u16 = 9120;
    let addr_a = format!("127.0.0.1:{port_a}");

    // In the self-contained demo we pick node-a's key ourselves so we can
    // compute its identity hash directly, rather than needing an operator to
    // read it off node-a's log. Node A's own privacy guarantees are unaffected either way:
    // the hash is meant to be public, only the underlying key is secret.
    let sk_a = pallas::Scalar::random(OsRng);
    let node_a_hash = hex32(&hs_hash(derive_pk(sk_a)).to_repr());

    thread::spawn(move || run_node_a_with_key(port_a, sk_a));
    thread::sleep(Duration::from_millis(150));

    println!("\nstep 1: Node B requests RDV status, submits a certificate, then the private routing instruction");
    let cert = request_rdv(&addr_a, &node_a_hash, now_unix() + 86_400, None);

    thread::sleep(Duration::from_millis(200));

    println!("\nstep 2: save the certificate and re-verify it completely standalone");
    let path = std::env::temp_dir().join("poc10_test_cert.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&cert).unwrap()).unwrap();
    verify_cert_file(path.to_str().unwrap());

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
            request_rdv(addr, hash, expires, target);
        }
        Some("verify") => {
            let path = args.get(2).expect("missing certificate file path");
            verify_cert_file(path);
        }
        Some("test") | None => run_test(),
        Some(other) => {
            eprintln!("unknown command: {other}");
            eprintln!("usage:");
            eprintln!("  hs-certificates node-a       [port]");
            eprintln!("  hs-certificates request-rdv  <node-a-addr> <node-a-hash-hex> [expires-unix-ts] [target-node-hash-hex]");
            eprintln!("  hs-certificates verify       <cert-file>");
            eprintln!("  hs-certificates test");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A one-day grant, priced accordingly (one day = MIN_CHALLENGE_DIFFICULTY).
    fn setup() -> (pallas::Scalar, [u8; 32], u128, u64) {
        let sk_a = pallas::Scalar::random(OsRng);
        let node_hash: [u8; 32] = hs_hash(derive_pk(sk_a)).to_repr();
        let expires = now_unix() + 86_400;
        let challenge = create_challenge(required_effort(86_400));
        (sk_a, node_hash, challenge, expires)
    }

    #[test]
    fn effort_scales_linearly_with_requested_lifetime() {
        assert_eq!(required_effort(86_400), MIN_CHALLENGE_DIFFICULTY);
        // Partial days round up: 25h costs 2 days.
        assert_eq!(required_effort(86_400 + 3_600), 2 * MIN_CHALLENGE_DIFFICULTY);
        assert_eq!(required_effort(30 * 86_400), 30 * MIN_CHALLENGE_DIFFICULTY);
        // Degenerate zero-length request still costs a day, never a free challenge.
        assert_eq!(required_effort(0), MIN_CHALLENGE_DIFFICULTY);
    }

    #[test]
    fn valid_certificate_accepted() {
        let (_, node_hash, challenge, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert = build_certificate(sk_b, node_hash, expires, challenge, solution);

        assert!(accept_certificate(&cert, node_hash, challenge, expires));
    }

    #[test]
    fn replayed_pow_solution_from_different_challenge_rejected() {
        // A solution that solves some Equi-X challenge, but not
        // the one Node A actually issued this session, must not be reusable.
        let (_, node_hash, challenge, expires) = setup();
        let other_challenge = create_challenge(MIN_CHALLENGE_DIFFICULTY);
        let solution_for_other_challenge = solve_challenge(1, other_challenge);

        let sk_b = pallas::Scalar::random(OsRng);
        // Certificate honestly claims other_challenge, not the one Node A issued.
        let cert = build_certificate(
            sk_b,
            node_hash,
            expires,
            other_challenge,
            solution_for_other_challenge,
        );

        assert!(!accept_certificate(&cert, node_hash, challenge, expires));
    }

    #[test]
    fn forged_pow_solution_rejected() {
        let (_, node_hash, challenge, expires) = setup();
        let sk_b = pallas::Scalar::random(OsRng);
        // Garbage solution bytes, never actually solved.
        let cert = build_certificate(sk_b, node_hash, expires, challenge, [0u8; 24]);

        assert!(!accept_certificate(&cert, node_hash, challenge, expires));
    }

    #[test]
    fn tampered_rdv_node_hash_rejected() {
        let (_, node_hash, challenge, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let mut cert = build_certificate(sk_b, node_hash, expires, challenge, solution);

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
        let (_, node_hash, challenge, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert = build_certificate(sk_b, node_hash, expires + 7 * 86_400, challenge, solution);

        assert!(!accept_certificate(&cert, node_hash, challenge, expires));
    }

    #[test]
    fn full_certificate_with_endorsement_verifies_standalone() {
        let (sk_a, node_hash, challenge, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let mut cert = build_certificate(sk_b, node_hash, expires, challenge, solution);
        assert!(accept_certificate(&cert, node_hash, challenge, expires));
        cert.endorsement = Some(endorse_certificate(sk_a, &cert));

        // Standalone re-verification, what a third party holding
        // only the certificate can and should check.
        let effort = get_challenge_effort(cert.pow_challenge);
        assert!(effort >= MIN_CHALLENGE_DIFFICULTY);
        assert!(verify_solution(effort, cert.pow_challenge, cert.pow_solution));
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
        let (_, node_hash, challenge, _) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        // Already-expired timestamp, with the requested expires matching so
        // it's specifically the expiry check that rejects.
        let expired = now_unix() - 3600;
        let cert = build_certificate(sk_b, node_hash, expired, challenge, solution);

        assert!(!accept_certificate(&cert, node_hash, challenge, expired));
    }

    #[test]
    fn certificate_beyond_max_lifetime_rejected() {
        let (_, node_hash, challenge, _) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        // Even if the request phase were bypassed, the acceptance check
        // itself refuses grants past the cap.
        let too_far = now_unix() + MAX_CERT_LIFETIME_SECS + 3600;
        let cert = build_certificate(sk_b, node_hash, too_far, challenge, solution);

        assert!(!accept_certificate(&cert, node_hash, challenge, too_far));
    }

    #[test]
    fn valid_routing_instruction_accepted() {
        let (_, node_hash, challenge, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert = build_certificate(sk_b, node_hash, expires, challenge, solution);
        assert!(accept_certificate(&cert, node_hash, challenge, expires));

        // Same sk as the certificate: this is the honest flow.
        let target = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();
        let instr = build_routing_instruction(sk_b, node_hash, target);
        assert!(accept_instruction(&instr, cert.hs_hash, node_hash));
    }

    #[test]
    fn routing_instruction_from_different_key_rejected() {
        // An attacker who saw a (public) certificate for hs_hash H tries to
        // attach its own routing instruction to that grant. Its instruction's
        // hs_hash can only ever be its OWN key's hash (the circuit binds
        // them), which won't match the granted H.
        let (_, node_hash, challenge, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert = build_certificate(sk_b, node_hash, expires, challenge, solution);

        let sk_attacker = pallas::Scalar::random(OsRng);
        let target = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();
        let instr = build_routing_instruction(sk_attacker, node_hash, target);
        assert!(!accept_instruction(&instr, cert.hs_hash, node_hash));

        // And simply overwriting the claimed hs_hash breaks the proof.
        let mut forged = instr;
        forged.hs_hash = cert.hs_hash;
        assert!(!accept_instruction(&forged, cert.hs_hash, node_hash));
    }

    #[test]
    fn routing_instruction_for_wrong_rdv_node_rejected() {
        // An instruction made out to some other RDV node can't be submitted
        // to this one, even by the legitimate HS.
        let (_, node_hash, challenge, expires) = setup();
        let solution = solve_challenge(1, challenge);
        let sk_b = pallas::Scalar::random(OsRng);
        let cert = build_certificate(sk_b, node_hash, expires, challenge, solution);

        let other_node = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();
        let target = hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr();
        let instr = build_routing_instruction(sk_b, other_node, target);
        assert!(!accept_instruction(&instr, cert.hs_hash, node_hash));
    }
}
