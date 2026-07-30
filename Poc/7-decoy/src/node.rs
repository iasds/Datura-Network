use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;

use x_wing::DecapsulationKey;

use crate::envelope::{self, PACKET_SIZE};

// one of the 8 endpoints in the decoy set. one is the real hs (running its own identity key); the other 7 are independent nodes
// holding their own unrelated keys, hs-sealed traffic fails to open for them.
pub struct Destination {
    pub slot: usize,
    pub secret: DecapsulationKey,
}

// one pkt as a dest saw it. many of these come back per run now (one per
// client per dest).
pub struct PacketSeen {
    pub slot: usize,
    pub wire_tag: String, // blake3 of raw bytes, first 8 hex
    pub opened: bool,     // true -> decrypted, false -> discarded as decoy
}

// tag = first 8 hex of blake3, enough for check, but short too
pub fn wire_tag(bytes: &[u8]) -> String {
    hex::encode(&blake3::hash(bytes).as_bytes()[..4])
}

// dest listens for `expect` packets (one per client), tags + tries to open each, reports every one back.
pub fn run_destination(
    listener: TcpListener,
    dest: Destination,
    expect: usize,
    report: Sender<PacketSeen>,
) {
    for _ in 0..expect {
        let mut client_conn = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(_) => return,
        };

        let mut packet = [0u8; PACKET_SIZE];
        if client_conn.read_exact(&mut packet).is_err() {
            continue;
        }

        let _ = report.send(PacketSeen {
            slot: dest.slot,
            wire_tag: wire_tag(&packet),
            opened: envelope::open(&dest.secret, &packet).is_some(),
        });
    }
}

// blind relay. takes each pkt and copies to every dest slot. no decrypt, routers blind
pub fn run_router(listener: TcpListener, forward_to: Vec<u16>, expect: usize) {
    for _ in 0..expect {
        let mut client_conn = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(_) => return,
        };

        let mut packet = [0u8; PACKET_SIZE];
        if client_conn.read_exact(&mut packet).is_err() {
            continue;
        }

        for port in &forward_to {
            if let Ok(mut dest_conn) = TcpStream::connect(("127.0.0.1", *port)) {
                let _ = dest_conn.write_all(&packet);
            }
        }
    }
}
