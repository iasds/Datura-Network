use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use x_wing::{EncapsulationKey,DecapsulationKey};
use getrandom::getrandom;

use crate::envelope::{self, PACKET_SIZE};

/// Holds destination private key
pub struct Destination {
    pub secret: DecapsulationKey,
}


/// One noise packet as observed by the RDV destination (Node C).
pub struct NoisePacketSeen {
    pub _seq: u32,
    pub _wire_tag: String,
    pub opened: bool,
}

/// tag = first 8 hex of blake3, enough for check, yet short enough
pub fn wire_tag(bytes: &[u8]) -> String {
    hex::encode(&blake3::hash(bytes).as_bytes()[..4])
}

/// Node C: RDV destination for noise packets from a Decoy Source.
/// Reads exactly `packet_size` bytes per connection and reports whether it could open them.
pub fn run_noise_destination(
    listener: TcpListener,
    dest: Destination,
    packet_size: usize,
    expect: usize,
    report: Sender<NoisePacketSeen>,
) {
    for seq in 0..expect {
        let mut client_conn = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(_) => return,
        };

        let mut packet = vec![0u8; packet_size];
        if client_conn.read_exact(&mut packet).is_err() {
            continue;
        }

        let opened = if packet.len() == PACKET_SIZE {
            envelope::open(&dest.secret, &packet).is_some()
        } else {
            false
        };

        let _ = report.send(NoisePacketSeen {
            _seq: seq.try_into().unwrap(),
            _wire_tag: wire_tag(&packet),
            opened,
        });
    }
}

/// Node B: Decoy Source. Accepts one sealed instruction from Node A, opens it,
/// then generates and transmits `packet_count` noise packets of `packet_size` bytes
/// to Node C at the requested bitrate.
pub fn run_decoy_source(
    listener: TcpListener,
    own_secret: DecapsulationKey,
    noise_target_port: u16,
    done: Sender<()>,
    report: Sender<NoisePacketSeen>,
) {
    let mut client_conn = match listener.accept() {
        Ok((stream, _)) => stream,
        Err(_) => return,
    };

    let mut packet = [0u8; PACKET_SIZE];
    if client_conn.read_exact(&mut packet).is_err() {
        return;
    }

    let opened = match envelope::open(&own_secret, &packet) {
        Some(bytes) => bytes,
        None => return,
    };

    let instruction = match envelope::decode_instruction(&opened) {
        Ok(inst) => inst,
        Err(_) => return,
    };

    let interval_ns = (instruction.packet_size as u64 * 8 * 1_000_000_000)
        .div_ceil(instruction.bitrate_bps);
    let interval = Duration::from_nanos(interval_ns.max(1));

    for seq in 0..instruction.packet_count {
        let mut noise = vec![0u8; instruction.packet_size as usize];
        if getrandom(&mut noise).is_err() {
            continue;
        }

        if let Ok(mut dest_conn) = TcpStream::connect(("127.0.0.1", noise_target_port)) {
            let _ = dest_conn.write_all(&noise);
        }

        let _ = report.send(NoisePacketSeen {
            _seq: seq,
            _wire_tag: wire_tag(&noise),
            opened: false,
        });

        if seq < instruction.packet_count - 1 {
            thread::sleep(interval);
        }
    }

    let _ = done.send(());
}

/// Node A seals an instruction and sends it to a Decoy Source (Node B).
/// The control packet is exactly PACKET_SIZE bytes and indistinguishable from ordinary traffic.
pub fn send_instructions(
    b_public: &EncapsulationKey,
    instruction: &envelope::DecoySourceInstruction,
    dest_port: u16,
) -> Result<String> {
    use std::io::Write;
    let payload = envelope::encode_instruction(instruction)?;
    let packet = envelope::seal(b_public, &payload)?;
    let mut sock = TcpStream::connect(("127.0.0.1", dest_port))
        .with_context(|| format!("client dialing decoy source at {dest_port}"))?;
    sock.write_all(&packet)?;
    Ok(wire_tag(&packet))
}