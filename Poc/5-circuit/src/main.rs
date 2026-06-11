/// PoC 5 — Datura 3-hop circuit building with hidden service rendezvous
///
/// Implements the protocol specified in spec/formal_spec/CircuitBuild.tla:
///   - AddCircuit:           client builds a 3-hop circuit to a destination
///   - SelectIntroPoint:     hidden service registers with the intro point
///   - ConnectHiddenService: client-->intro→HS, HS-->RV, RV bridges the two legs //?
///
/// Usage:
///   circuit relay     <port>
///   circuit client    <hop1> <hop2> <dest> <message>
///   circuit hs        <port> <intro_addr>
///   circuit client-hs <hop1> <hop2> <rv_relay> <intro> <message> //?
///   circuit test
///
/// Wire: fixed 512-byte cells [type:1][circuit_id:4][payload:507]
/// Crypto: ephemeral X25519 DH → HKDF-SHA256 → ChaCha20 stream cipher per hop
///
/// Circuit extension uses EXTEND/EXTENDED cells (no AEAD tag → no size expansion).
/// Relay RELAY cells use ChaCha20 counter mode: each hop XORs one keystream layer,
/// payload stays exactly PAYLOAD_LEN bytes throughout.

mod crypto;
mod proto;

use crypto::{complete_dh, gen_keypair, StreamKeys, PUBKEY_LEN};
use proto::{Cell, PAYLOAD_LEN};
use proto::{
    TYPE_BRIDGE, TYPE_CONNECT, TYPE_CREATE, TYPE_CREATED, TYPE_DATA, //?
    TYPE_EXTEND, TYPE_EXTENDED, TYPE_INTRO, TYPE_RELAY, TYPE_RENDEZVOUS,
};

use std::collections::HashMap;
use std::io::Write; //?
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const IO_TIMEOUT: Duration = Duration::from_secs(30);

// ── Relay node ────────────────────────────────────────────────────────────────

struct CircuitEntry {
    keys: StreamKeys,
    next: Option<Arc<Mutex<TcpStream>>>,      // downstream: forward RELAY to next hop
    upstream: Arc<Mutex<TcpStream>>,           // the accepted stream; used by BRIDGE to write back //?
    bridge_to: Option<Arc<Mutex<TcpStream>>>, // when set, forward decrypted DATA here instead of printing //?
}

type CircuitTable = Arc<Mutex<HashMap<u32, CircuitEntry>>>;

// Maps HS circuit_id → try_clone of the HS's upstream stream. //?
// Written by TYPE_INTRO; read by TYPE_CONNECT to forward RENDEZVOUS to the HS. //?
type IntroRegistrations = Arc<Mutex<HashMap<u32, Arc<Mutex<TcpStream>>>>>; //?

fn run_relay(port: u16) {
    let circuits: CircuitTable = Arc::new(Mutex::new(HashMap::new()));
    let intro_regs: IntroRegistrations = Arc::new(Mutex::new(HashMap::new())); //?
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).expect("relay bind failed");
    println!("[relay:{}] listening", port);

    for incoming in listener.incoming() {
        let stream = incoming.expect("accept failed");
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
        let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        let circuits = circuits.clone();
        let intro_regs = intro_regs.clone(); //?
        thread::spawn(move || relay_conn(stream, peer, circuits, intro_regs)); //?
    }
}

fn relay_conn(
    mut stream: TcpStream,
    peer: String,
    circuits: CircuitTable,
    intro_regs: IntroRegistrations, //?
) {
    loop {
        let cell = match Cell::recv(&mut stream) {
            Ok(c) => c,
            Err(_) => break,
        };

        match cell.cell_type {
            // ── CREATE: begin DH handshake for this hop ───────────────────────
            TYPE_CREATE => {
                let mut client_pub = [0u8; PUBKEY_LEN];
                client_pub.copy_from_slice(&cell.payload[..PUBKEY_LEN]);

                let (relay_sec, relay_pub) = gen_keypair();
                let shared = complete_dh(relay_sec, &client_pub);
                let keys = StreamKeys::from_shared(&shared);
                let upstream = Arc::new(Mutex::new(stream.try_clone().unwrap())); //?

                circuits.lock().unwrap().insert(
                    cell.circuit_id,
                    CircuitEntry { keys, next: None, upstream, bridge_to: None }, //?
                );

                let mut created = Cell::new(TYPE_CREATED, cell.circuit_id);
                created.payload[..PUBKEY_LEN].copy_from_slice(&relay_pub);
                created.send(&mut stream).unwrap_or(());
                println!("[relay:{}] CREATE circuit={} established", peer, cell.circuit_id);
            }

            // ── EXTEND: client asks this hop to extend the circuit one hop ────
            // payload: [addr_len:1][addr:N][client_pubkey_for_next:32]
            // If we already have a downstream, forward the EXTEND there.
            // Otherwise connect to addr, do CREATE/CREATED, return EXTENDED.
            TYPE_EXTEND => {
                let addr_len = cell.payload[0] as usize;
                let addr =
                    String::from_utf8_lossy(&cell.payload[1..1 + addr_len]).to_string();
                let mut their_pub = [0u8; PUBKEY_LEN];
                their_pub.copy_from_slice(
                    &cell.payload[1 + addr_len..1 + addr_len + PUBKEY_LEN],
                );

                let next_opt = circuits
                    .lock()
                    .unwrap()
                    .get(&cell.circuit_id)
                    .and_then(|e| e.next.clone());

                if let Some(next_arc) = next_opt {
                    // Forward EXTEND to downstream; relay EXTENDED back upstream.
                    let mut fwd = next_arc.lock().unwrap();
                    if cell.send(&mut fwd).is_err() { break; }
                    match Cell::recv(&mut fwd) {
                        Ok(ext) => { drop(fwd); ext.send(&mut stream).unwrap_or(()); }
                        Err(_) => break,
                    }
                } else {
                    // Connect to addr, do CREATE/CREATED, store downstream.
                    match TcpStream::connect(&addr) {
                        Ok(mut ns) => {
                            ns.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
                            ns.set_write_timeout(Some(IO_TIMEOUT)).unwrap();
                            let mut create_fwd = Cell::new(TYPE_CREATE, cell.circuit_id);
                            create_fwd.payload[..PUBKEY_LEN].copy_from_slice(&their_pub);
                            if create_fwd.send(&mut ns).is_err() { break; }
                            match Cell::recv(&mut ns) {
                                Ok(created) => {
                                    let arc_ns = Arc::new(Mutex::new(ns));
                                    circuits.lock().unwrap()
                                        .entry(cell.circuit_id)
                                        .and_modify(|e| e.next = Some(arc_ns));
                                    let mut ext = Cell::new(TYPE_EXTENDED, cell.circuit_id);
                                    ext.payload[..PUBKEY_LEN]
                                        .copy_from_slice(&created.payload[..PUBKEY_LEN]);
                                    ext.send(&mut stream).unwrap_or(());
                                    println!("[relay:{}] EXTEND circuit={} → {}", peer, cell.circuit_id, addr);
                                }
                                Err(_) => break,
                            }
                        }
                        Err(e) => {
                            eprintln!("[relay] EXTEND connect to {} failed: {}", addr, e);
                            break;
                        }
                    }
                }
            }

            // ── RELAY: peel one onion layer; forward or deliver ───────────────
            TYPE_RELAY => {
                let result = {
                    let mut guard = circuits.lock().unwrap();
                    match guard.get_mut(&cell.circuit_id) {
                        Some(entry) => {
                            let mut payload = cell.payload;
                            entry.keys.apply_fwd(&mut payload);
                            Some((payload, entry.next.clone(), entry.bridge_to.clone())) //?
                        }
                        None => None,
                    }
                };

                match result {
                    None => {
                        eprintln!("[relay] unknown circuit {}", cell.circuit_id);
                        break;
                    }
                    Some((payload, Some(next_arc), _)) => {
                        let mut fwd_cell = Cell::new(TYPE_RELAY, cell.circuit_id);
                        fwd_cell.payload = payload;
                        let mut ns = next_arc.lock().unwrap();
                        fwd_cell.send(&mut ns).unwrap_or(());
                    }
                    Some((payload, None, bridge_to)) => { //?
                        // Terminal hop: inner payload starts with the data cell type.
                        let inner_type = payload[0];
                        if inner_type == TYPE_DATA {
                            let len = u16::from_le_bytes(
                                payload[1..3].try_into().unwrap()
                            ) as usize;
                            if len > PAYLOAD_LEN - 3 { //?
                                eprintln!("[relay:{}] bad DATA len {}", peer, len); //?
                            } else if let Some(arc) = bridge_to { //?
                                // Bridge is active: forward decrypted inner payload to the HS.
                                let mut fwd = Cell::new(TYPE_DATA, cell.circuit_id); //?
                                fwd.payload.copy_from_slice(&payload); //?
                                arc.lock().unwrap().write_all(&fwd.to_bytes()).ok(); //?
                            } else {
                                let msg = String::from_utf8_lossy(&payload[3..3 + len]);
                                println!("[relay:{}] DATA circuit={}: {}", peer, cell.circuit_id, msg);
                            }
                        }
                    }
                }
            }

            // ── BRIDGE: HS tells the RV relay to link its circuit to the client's ──
            // payload: [client_circuit_id:4 LE] //?
            // Looks up the client circuit and sets bridge_to = this HS's upstream stream, //?
            // so decrypted DATA from the client gets forwarded to the HS. //?
            TYPE_BRIDGE => { //?
                let client_cid = u32::from_le_bytes(cell.payload[..4].try_into().unwrap()); //?
                println!("[relay:{}] BRIDGE HS circuit={} ↔ client circuit={}", peer, cell.circuit_id, client_cid); //?
                //?
                let mut table = circuits.lock().unwrap(); //?
                let hs_upstream = table.get(&cell.circuit_id).map(|e| e.upstream.clone()); //?
                if let Some(hu) = hs_upstream { //?
                    if let Some(client_entry) = table.get_mut(&client_cid) { //?
                        client_entry.bridge_to = Some(hu); //?
                        println!("[relay:{}] bridge active; data will flow to HS", peer); //?
                    } else { //?
                        eprintln!("[relay] BRIDGE: client circuit {} not found", client_cid); //?
                    } //?
                } else { //?
                    eprintln!("[relay] BRIDGE: HS circuit {} not found", cell.circuit_id); //?
                } //?
            } //?

            // ── INTRO: hidden service registers with this intro point ─────────
            // payload: [addr_len:1][hs_addr:N]  (hs_addr is informational; not forwarded)
            TYPE_INTRO => {
                let addr_len = cell.payload[0] as usize;
                let hs_addr =
                    String::from_utf8_lossy(&cell.payload[1..1 + addr_len]).to_string();
                let hs_stream = Arc::new(Mutex::new(stream.try_clone().unwrap())); //?
                intro_regs.lock().unwrap().insert(cell.circuit_id, hs_stream); //?
                println!("[intro:{}] HS registered on circuit {} (addr: {})", peer, cell.circuit_id, hs_addr); //?
            }

            // ── CONNECT: client asks intro to signal an HS on their behalf ──── //?
            // payload: [addr_len:1][rv_addr:N][client_circuit_id:4 LE] //?
            // Intro picks the first registered HS and forwards a RENDEZVOUS cell. //?
            TYPE_CONNECT => { //?
                let addr_len = cell.payload[0] as usize; //?
                let rv_addr = //?
                    String::from_utf8_lossy(&cell.payload[1..1 + addr_len]).to_string(); //?
                let cid_offset = 1 + addr_len; //?
                let client_cid = u32::from_le_bytes( //?
                    cell.payload[cid_offset..cid_offset + 4].try_into().unwrap(), //?
                ); //?
                println!("[intro:{}] CONNECT: client at RV {} circuit {}", peer, rv_addr, client_cid); //?
                //?
                let regs = intro_regs.lock().unwrap(); //?
                if let Some((&hs_cid, hs_stream)) = regs.iter().next() { //?
                    let hs_stream = hs_stream.clone(); //?
                    drop(regs); //?
                    let mut rv_cell = Cell::new(TYPE_RENDEZVOUS, hs_cid); //?
                    rv_cell.payload[0] = addr_len as u8; //?
                    rv_cell.payload[1..1 + addr_len].copy_from_slice(rv_addr.as_bytes()); //?
                    rv_cell.payload[1 + addr_len..5 + addr_len] //?
                        .copy_from_slice(&client_cid.to_le_bytes()); //?
                    hs_stream.lock().unwrap().write_all(&rv_cell.to_bytes()).ok(); //?
                    println!("[intro:{}] RENDEZVOUS forwarded to HS circuit {}", peer, hs_cid); //?
                } else { //?
                    eprintln!("[intro] CONNECT: no HS registered"); //?
                } //?
            } //?

            other => {
                eprintln!("[relay] unknown cell type 0x{:02x}", other);
                break;
            }
        }
    }
}

// ── Client: build 3-hop circuit and send data ────────────────────────────────

/// Build a 3-hop circuit: client → hop1 → hop2 → dest, then send a DATA message.
///
/// Per CircuitBuild.tla (AddCircuit):
///   1. CREATE  → hop1: DH handshake for leg 1                  (direct)
///   2. EXTEND  → hop1: "connect me to hop2"   [hop2_addr][pub2]
///      hop1 → hop2: CREATE [pub2]; hop2 → hop1: CREATED; hop1 → client: EXTENDED
///   3. EXTEND  → hop1: "connect me to dest"   [dest_addr][pub3]
///      hop1 → hop2: EXTEND; hop2 → dest: CREATE; dest → hop2 → hop1 → client: EXTENDED
///   4. RELAY   → hop1 → hop2 → dest: triple-onion-encrypted DATA
fn run_client(hop1: &str, hop2: &str, dest: &str, message: &str) {
    println!("[client] building circuit: {} → {} → {}", hop1, hop2, dest);
    let circuit_id: u32 = rand::random();

    let mut s = TcpStream::connect(hop1).expect("connect hop1");
    s.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
    s.set_write_timeout(Some(IO_TIMEOUT)).unwrap();

    // ── Leg 1: CREATE ↔ hop1 ─────────────────────────────────────────────────
    let (sec1, pub1) = gen_keypair();
    let mut c1 = Cell::new(TYPE_CREATE, circuit_id);
    c1.payload[..PUBKEY_LEN].copy_from_slice(&pub1);
    c1.send(&mut s).unwrap();

    let cr1 = Cell::recv(&mut s).expect("CREATED from hop1");
    assert_eq!(cr1.cell_type, TYPE_CREATED);
    let mut h1_pub = [0u8; PUBKEY_LEN];
    h1_pub.copy_from_slice(&cr1.payload[..PUBKEY_LEN]);
    let mut keys1 = StreamKeys::from_shared(&complete_dh(sec1, &h1_pub));
    println!("[client] ✓ leg 1 (hop1)");

    // ── Leg 2: EXTEND through hop1 to hop2 ───────────────────────────────────
    let (sec2, pub2) = gen_keypair();
    let (_, mut keys2) = extend_circuit(&mut s, circuit_id, hop2, pub2, sec2); //?
    println!("[client] ✓ leg 2 (hop2)");

    // ── Leg 3: EXTEND through hop1→hop2 to dest ──────────────────────────────
    let (sec3, pub3) = gen_keypair();
    let (_, mut keys3) = extend_circuit(&mut s, circuit_id, dest, pub3, sec3); //?
    println!("[client] ✓ leg 3 (dest)");

    // ── Send DATA triple-encrypted (inner = dest key, outer = hop1 key) ──────
    let msg = message.as_bytes();
    assert!(msg.len() <= PAYLOAD_LEN - 3, "message too long: {} bytes, max {}", msg.len(), PAYLOAD_LEN - 3); //?
    let mut payload = [0u8; PAYLOAD_LEN];
    payload[0] = TYPE_DATA;
    payload[1..3].copy_from_slice(&(msg.len() as u16).to_le_bytes());
    payload[3..3 + msg.len()].copy_from_slice(msg);

    keys3.apply_fwd(&mut payload); // innermost layer (dest)
    keys2.apply_fwd(&mut payload); // middle layer   (hop2)
    keys1.apply_fwd(&mut payload); // outermost layer (hop1)

    let mut relay = Cell::new(TYPE_RELAY, circuit_id);
    relay.payload = payload;
    relay.send(&mut s).unwrap();

    println!("[client] ✓ DATA sent through 3-hop circuit: \"{}\"", message);
}

// Helper: send EXTEND cell and return (their_pubkey, StreamKeys). //?
fn extend_circuit( //?
    s: &mut TcpStream, //?
    circuit_id: u32, //?
    addr: &str, //?
    our_pub: [u8; PUBKEY_LEN], //?
    our_sec: x25519_dalek::EphemeralSecret, //?
) -> ([u8; PUBKEY_LEN], StreamKeys) { //?
    let addr_bytes = addr.as_bytes(); //?
    let mut ext = Cell::new(TYPE_EXTEND, circuit_id); //?
    ext.payload[0] = addr_bytes.len() as u8; //?
    ext.payload[1..1 + addr_bytes.len()].copy_from_slice(addr_bytes); //?
    ext.payload[1 + addr_bytes.len()..1 + addr_bytes.len() + PUBKEY_LEN] //?
        .copy_from_slice(&our_pub); //?
    ext.send(s).unwrap(); //?
    //?
    let extended = Cell::recv(s).expect("EXTENDED"); //?
    assert_eq!(extended.cell_type, TYPE_EXTENDED, "expected EXTENDED"); //?
    let mut their_pub = [0u8; PUBKEY_LEN]; //?
    their_pub.copy_from_slice(&extended.payload[..PUBKEY_LEN]); //?
    let shared = complete_dh(our_sec, &their_pub); //?
    (their_pub, StreamKeys::from_shared(&shared)) //?
} //?

// ── Client-HS: build circuit to RV relay, then signal intro to connect ─────── //?

/// Build a 3-hop circuit ending at the RV relay, then tell the intro point //?
/// to connect us to a registered hidden service. //?
/// //?
/// Flow (ConnectHiddenService in CircuitBuild.tla): //?
///   1. Build 3-hop circuit: hop1 → hop2 → rv_relay  (our circuit_id) //?
///   2. Send TYPE_CONNECT directly to intro: [rv_relay_addr][circuit_id] //?
///   3. Intro looks up registered HS and forwards TYPE_RENDEZVOUS to it //?
///   4. HS connects to rv_relay, does CREATE/CREATED, sends TYPE_BRIDGE [circuit_id] //?
///   5. rv_relay sets client circuit's bridge_to = HS upstream stream //?
///   6. Send DATA through circuit; rv_relay decrypts and forwards to HS //?
fn run_client_hs(hop1: &str, hop2: &str, rv: &str, intro: &str, message: &str) { //?
    println!("[client-hs] building circuit to RV: {} → {} → {}", hop1, hop2, rv); //?
    let circuit_id: u32 = rand::random(); //?
    //?
    let mut s = TcpStream::connect(hop1).expect("connect hop1"); //?
    s.set_read_timeout(Some(IO_TIMEOUT)).unwrap(); //?
    s.set_write_timeout(Some(IO_TIMEOUT)).unwrap(); //?
    //?
    // ── Leg 1 ──────────────────────────────────────────────────────────────── //?
    let (sec1, pub1) = gen_keypair(); //?
    let mut c1 = Cell::new(TYPE_CREATE, circuit_id); //?
    c1.payload[..PUBKEY_LEN].copy_from_slice(&pub1); //?
    c1.send(&mut s).unwrap(); //?
    //?
    let cr1 = Cell::recv(&mut s).expect("CREATED from hop1"); //?
    assert_eq!(cr1.cell_type, TYPE_CREATED); //?
    let mut h1_pub = [0u8; PUBKEY_LEN]; //?
    h1_pub.copy_from_slice(&cr1.payload[..PUBKEY_LEN]); //?
    let mut keys1 = StreamKeys::from_shared(&complete_dh(sec1, &h1_pub)); //?
    println!("[client-hs] ✓ leg 1 (hop1)"); //?
    //?
    // ── Leg 2 ──────────────────────────────────────────────────────────────── //?
    let (sec2, pub2) = gen_keypair(); //?
    let (_, mut keys2) = extend_circuit(&mut s, circuit_id, hop2, pub2, sec2); //?
    println!("[client-hs] ✓ leg 2 (hop2)"); //?
    //?
    // ── Leg 3: RV relay is the terminal hop ────────────────────────────────── //?
    let (sec3, pub3) = gen_keypair(); //?
    let (_, mut keys3) = extend_circuit(&mut s, circuit_id, rv, pub3, sec3); //?
    println!("[client-hs] ✓ leg 3 (RV relay)"); //?
    //?
    // ── Signal intro point: I'm waiting at rv on circuit_id ────────────────── //?
    let mut intro_conn = TcpStream::connect(intro).expect("connect intro"); //?
    intro_conn.set_read_timeout(Some(IO_TIMEOUT)).unwrap(); //?
    intro_conn.set_write_timeout(Some(IO_TIMEOUT)).unwrap(); //?
    //?
    let rv_bytes = rv.as_bytes(); //?
    let mut connect_cell = Cell::new(TYPE_CONNECT, circuit_id); //?
    connect_cell.payload[0] = rv_bytes.len() as u8; //?
    connect_cell.payload[1..1 + rv_bytes.len()].copy_from_slice(rv_bytes); //?
    connect_cell.payload[1 + rv_bytes.len()..5 + rv_bytes.len()] //?
        .copy_from_slice(&circuit_id.to_le_bytes()); //?
    connect_cell.send(&mut intro_conn).unwrap(); //?
    println!("[client-hs] ✓ TYPE_CONNECT sent to intro; waiting for HS to bridge..."); //?
    //?
    // Give the HS time to receive RENDEZVOUS, connect to the RV, and send BRIDGE. //?
    thread::sleep(Duration::from_millis(500)); //?
    //?
    // ── Send DATA through the circuit; RV will forward to HS via bridge ────── //?
    let msg = message.as_bytes(); //?
    assert!(msg.len() <= PAYLOAD_LEN - 3, "message too long: {} bytes, max {}", msg.len(), PAYLOAD_LEN - 3); //?
    let mut payload = [0u8; PAYLOAD_LEN]; //?
    payload[0] = TYPE_DATA; //?
    payload[1..3].copy_from_slice(&(msg.len() as u16).to_le_bytes()); //?
    payload[3..3 + msg.len()].copy_from_slice(msg); //?
    //?
    keys3.apply_fwd(&mut payload); //?
    keys2.apply_fwd(&mut payload); //?
    keys1.apply_fwd(&mut payload); //?
    //?
    let mut relay = Cell::new(TYPE_RELAY, circuit_id); //?
    relay.payload = payload; //?
    relay.send(&mut s).unwrap(); //?
    //?
    println!("[client-hs] ✓ DATA sent; HS should receive: \"{}\"", message); //?
} //?

// ── Hidden Service ────────────────────────────────────────────────────────────

/// Run as a hidden service (SelectIntroPoint / ConnectHiddenService in the TLA+ spec):
///   1. Connect to intro_addr and send INTRO cell to register.
///   2. Listen for RENDEZVOUS cells from the intro carrying a client's RV relay + circuit_id.
///   3. Connect to the RV relay, do CREATE/CREATED, then send BRIDGE [client_circuit_id].
///   4. Read DATA cells forwarded by the RV relay and print them.
fn run_hidden_service(port: u16, intro_addr: &str) {
    println!("[hs] registering with intro point {}", intro_addr);
    let hs_addr = format!("127.0.0.1:{}", port); //?
    let circuit_id: u32 = rand::random();

    // Connect to intro point and register.
    let mut intro_conn = TcpStream::connect(intro_addr).expect("connect intro");
    intro_conn.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
    intro_conn.set_write_timeout(Some(IO_TIMEOUT)).unwrap();

    let (sec, pub_bytes) = gen_keypair(); //?
    let mut c = Cell::new(TYPE_CREATE, circuit_id); //?
    c.payload[..PUBKEY_LEN].copy_from_slice(&pub_bytes); //?
    c.send(&mut intro_conn).unwrap(); //?
    let cr = Cell::recv(&mut intro_conn).expect("CREATED from intro"); //?
    let mut intro_pub = [0u8; PUBKEY_LEN]; //?
    intro_pub.copy_from_slice(&cr.payload[..PUBKEY_LEN]); //?
    let _keys = StreamKeys::from_shared(&complete_dh(sec, &intro_pub)); //?

    let hs_bytes = hs_addr.as_bytes();
    let mut intro_cell = Cell::new(TYPE_INTRO, circuit_id);
    intro_cell.payload[0] = hs_bytes.len() as u8;
    intro_cell.payload[1..1 + hs_bytes.len()].copy_from_slice(hs_bytes);
    intro_cell.send(&mut intro_conn).unwrap();
    println!("[hs] registered with intro; waiting for clients...");

    // Listen for RENDEZVOUS cells (intro forwarding a client's RV info to us).
    loop {
        let cell = match Cell::recv(&mut intro_conn) {
            Ok(c) => c,
            Err(_) => break,
        };

        if cell.cell_type != TYPE_RENDEZVOUS {
            continue;
        }

        let addr_len = cell.payload[0] as usize; //?
        let rv_relay = String::from_utf8_lossy(&cell.payload[1..1 + addr_len]).to_string(); //?
        let cid_offset = 1 + addr_len; //?
        let client_cid = u32::from_le_bytes( //?
            cell.payload[cid_offset..cid_offset + 4].try_into().unwrap(), //?
        ); //?
        println!("[hs] client waiting at RV {} on circuit {}", rv_relay, client_cid); //?

        // Connect to the RV relay, establish a circuit, then send BRIDGE.
        if let Ok(mut rv_conn) = TcpStream::connect(&rv_relay) { //?
            rv_conn.set_read_timeout(Some(IO_TIMEOUT)).unwrap(); //?
            rv_conn.set_write_timeout(Some(IO_TIMEOUT)).unwrap(); //?

            let hs_cid: u32 = rand::random(); //?
            let (sec, pub_bytes) = gen_keypair(); //?
            let mut create = Cell::new(TYPE_CREATE, hs_cid); //?
            create.payload[..PUBKEY_LEN].copy_from_slice(&pub_bytes); //?
            create.send(&mut rv_conn).unwrap(); //?

            let created = Cell::recv(&mut rv_conn).expect("CREATED from RV"); //?
            let mut rv_pub = [0u8; PUBKEY_LEN]; //?
            rv_pub.copy_from_slice(&created.payload[..PUBKEY_LEN]); //?
            let _rv_keys = StreamKeys::from_shared(&complete_dh(sec, &rv_pub)); //?

            let mut bridge = Cell::new(TYPE_BRIDGE, hs_cid); //?
            bridge.payload[..4].copy_from_slice(&client_cid.to_le_bytes()); //?
            bridge.send(&mut rv_conn).unwrap_or(()); //?
            println!("[hs] BRIDGE sent; circuit complete"); //?

            // Read DATA cells forwarded by the RV relay from the bridged client.
            loop {
                match Cell::recv(&mut rv_conn) { //?
                    Ok(cell) if cell.cell_type == TYPE_DATA => { //?
                        let len = u16::from_le_bytes( //?
                            cell.payload[1..3].try_into().unwrap() //?
                        ) as usize; //?
                        if len <= PAYLOAD_LEN - 3 { //?
                            let msg = String::from_utf8_lossy(&cell.payload[3..3 + len]); //?
                            println!("[hs] received from client: {}", msg); //?
                        } //?
                    } //?
                    Ok(_) => {} //?
                    Err(_) => break, //?
                } //?
            } //?
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

fn run_tests() {
    test_dh_stream_roundtrip();
    test_onion_wrap_unwrap();
    test_full_relay_circuit();
    test_full_hs_rendezvous(); //?
    println!("\nAll tests passed.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dh_stream() { test_dh_stream_roundtrip(); }

    #[test]
    fn onion() { test_onion_wrap_unwrap(); }

    #[test]
    fn relay_circuit() { test_full_relay_circuit(); }

    #[test]
    fn hs_rendezvous() { test_full_hs_rendezvous(); } //?
}

fn test_dh_stream_roundtrip() {
    print!("test_dh_stream_roundtrip ... ");
    let (sc, pc) = gen_keypair();
    let (sh, ph) = gen_keypair();
    let shared_c = complete_dh(sc, &ph);
    let shared_h = complete_dh(sh, &pc);
    let mut client_keys = StreamKeys::from_shared(&shared_c);
    let mut relay_keys  = StreamKeys::from_shared(&shared_h);

    let mut buf = [0u8; PAYLOAD_LEN];
    buf[..12].copy_from_slice(b"hello datura");
    let original = buf;

    client_keys.apply_fwd(&mut buf);        // client encrypts
    relay_keys.apply_fwd(&mut buf);         // relay decrypts (same XOR operation)
    assert_eq!(buf, original, "fwd roundtrip");

    relay_keys.apply_bwd(&mut buf);         // relay encrypts reply
    client_keys.apply_bwd(&mut buf);        // client decrypts
    assert_eq!(buf, original, "bwd roundtrip");
    println!("ok");
}

fn test_onion_wrap_unwrap() {
    print!("test_onion_wrap_unwrap ... ");
    let mut client_keys: Vec<StreamKeys> = Vec::new();
    let mut relay_keys:  Vec<StreamKeys> = Vec::new();
    for _ in 0..3 {
        let (sc, pc) = gen_keypair();
        let (sh, ph) = gen_keypair();
        client_keys.push(StreamKeys::from_shared(&complete_dh(sc, &ph)));
        relay_keys.push(StreamKeys::from_shared(&complete_dh(sh, &pc)));
    }

    let msg = b"3-hop plaintext";
    let mut payload = [0u8; PAYLOAD_LEN];
    payload[0] = TYPE_DATA;
    payload[1..3].copy_from_slice(&(msg.len() as u16).to_le_bytes());
    payload[3..3 + msg.len()].copy_from_slice(msg);
    let original = payload;

    // Client wraps: innermost = dest (index 2), outermost = hop1 (index 0)
    client_keys[2].apply_fwd(&mut payload);
    client_keys[1].apply_fwd(&mut payload);
    client_keys[0].apply_fwd(&mut payload);

    // Each relay peels one layer
    relay_keys[0].apply_fwd(&mut payload);
    relay_keys[1].apply_fwd(&mut payload);
    relay_keys[2].apply_fwd(&mut payload);

    assert_eq!(payload, original, "onion peel mismatch");
    println!("ok");
}

fn test_full_relay_circuit() {
    print!("test_full_relay_circuit ... ");

    let ports: [u16; 3] = [19310, 19311, 19312];
    for &p in &ports {
        thread::spawn(move || run_relay(p));
    }
    thread::sleep(Duration::from_millis(150));

    // Run client in-process; success = no panic + DATA printed by relay.
    run_client(
        &format!("127.0.0.1:{}", ports[0]),
        &format!("127.0.0.1:{}", ports[1]),
        &format!("127.0.0.1:{}", ports[2]),
        "test_full_relay",
    );
    thread::sleep(Duration::from_millis(50));
    println!("ok");
}

fn test_full_hs_rendezvous() { //?
    print!("test_full_hs_rendezvous ... "); //?
    //?
    // Four relay nodes: hop1, hop2, rv relay, intro point. //?
    let [hop1_port, hop2_port, rv_port, intro_port]: [u16; 4] = [19320, 19321, 19322, 19323]; //?
    for &p in &[hop1_port, hop2_port, rv_port, intro_port] { //?
        thread::spawn(move || run_relay(p)); //?
    } //?
    thread::sleep(Duration::from_millis(200)); //?
    //?
    // Start hidden service: registers with intro on port 19323. //?
    // Port 19324 is the hs_addr sent in the INTRO payload (not actually listened on in this PoC). //?
    thread::spawn(|| run_hidden_service(19324, "127.0.0.1:19323")); //?
    thread::sleep(Duration::from_millis(200)); //?
    //?
    // Client connects to HS via intro + RV. //?
    run_client_hs( //?
        &format!("127.0.0.1:{}", hop1_port), //?
        &format!("127.0.0.1:{}", hop2_port), //?
        &format!("127.0.0.1:{}", rv_port), //?
        &format!("127.0.0.1:{}", intro_port), //?
        "test_hs_message", //?
    ); //?
    //?
    // Give the HS time to receive and print the message. //?
    thread::sleep(Duration::from_millis(300)); //?
    println!("ok"); //?
} //?

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("relay") => {
            let port: u16 = args.get(2).and_then(|p| p.parse().ok())
                .expect("usage: circuit relay <port>");
            run_relay(port);
        }
        Some("client") => {
            let hop1 = args.get(2).expect("usage: circuit client <hop1> <hop2> <dest> <msg>");
            let hop2 = args.get(3).expect("missing hop2");
            let dest = args.get(4).expect("missing dest");
            let msg  = args.get(5).map(|s| s.as_str()).unwrap_or("hello from datura client");
            run_client(hop1, hop2, dest, msg);
        }
        Some("client-hs") => { //?
            let hop1  = args.get(2).expect("usage: circuit client-hs <hop1> <hop2> <rv_relay> <intro> <msg>"); //?
            let hop2  = args.get(3).expect("missing hop2"); //?
            let rv    = args.get(4).expect("missing rv_relay"); //?
            let intro = args.get(5).expect("missing intro"); //?
            let msg   = args.get(6).map(|s| s.as_str()).unwrap_or("hello from datura client"); //?
            run_client_hs(hop1, hop2, rv, intro, msg); //?
        } //?
        Some("hs") => {
            let port: u16 = args.get(2).and_then(|p| p.parse().ok())
                .expect("usage: circuit hs <port> <intro_addr>");
            let intro = args.get(3).expect("missing intro_addr");
            run_hidden_service(port, intro);
        }
        Some("test") => run_tests(),
        _ => {
            eprintln!("usage:");
            eprintln!("  circuit relay     <port>");
            eprintln!("  circuit client    <hop1> <hop2> <dest> <message>");
            eprintln!("  circuit client-hs <hop1> <hop2> <rv_relay> <intro> <message>"); //?
            eprintln!("  circuit hs        <port> <intro_addr>");
            eprintln!("  circuit test");
            std::process::exit(1);
        }
    }
}
