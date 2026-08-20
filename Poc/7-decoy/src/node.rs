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

// one of the 6 relays in the fanout tree. blind, no decrypt, just forwards
// lvl1 relays to lvl2 relays to 8 dests
pub struct Relay {
    pub level: usize,
    pub index: usize,
}

// one pkt as seen by relay, shows hops occcured not just 8 endpoints
pub struct HopSeen {
    pub level: usize,
    pub index: usize,
    pub wire_tag: String,
    pub forwarded: usize, // how many next hops it copied to
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
        // a dest is fed by a hop2 relay, not from client
        let mut inbound = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(_) => return,
        };

        let mut packet = [0u8; PACKET_SIZE];
        if inbound.read_exact(&mut packet).is_err() {
            continue;
        }

        let _ = report.send(PacketSeen {
            slot: dest.slot,
            wire_tag: wire_tag(&packet),
            opened: envelope::open(&dest.secret, &packet).is_some(),
        });
    }
}

// blind relay. takes pkt and copies to next hops, no decrypt
// same fn at both lvls: it doesnt know if forward_to is relay or dest
pub fn run_relay(
    listener: TcpListener,
    relay: Relay,
    forward_to: Vec<u16>,
    expect: usize,
    report: Sender<HopSeen>,
) {
    for _ in 0..expect {
        // a hop1 relay is fed by a client, a hop2 relay by a hop1 relay. it cant tell, so neither name fits
        let mut inbound = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(_) => return,
        };

        let mut packet = [0u8; PACKET_SIZE];
        if inbound.read_exact(&mut packet).is_err() {
            continue;
        }

        let mut forwarded: usize = 0;
        for port in &forward_to {
            // next hop, maybe a relay or dest
            if let Ok(mut outbound) = TcpStream::connect(("127.0.0.1", *port))
                && outbound.write_all(&packet).is_ok()
            {
                forwarded += 1;
            }
        }

        let _ = report.send(HopSeen {
            level: relay.level,
            index: relay.index,
            wire_tag: wire_tag(&packet),
            forwarded,
        });
    }
}
