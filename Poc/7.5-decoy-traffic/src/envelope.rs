use anyhow::{Result, bail};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit};
use x_wing::{Ciphertext, Decapsulate, DecapsulationKey, Encapsulate, EncapsulationKey};

// x-wing ct fixed 1120, everything after is one aead ciphertext of a
// fixed-size inner plaintext, so whole pkt is constant sized
pub const PACKET_SIZE: usize = 2048;

const CT_END: usize = 1120; // x_wing::CIPHERTEXT_SIZE
const NONCE_END: usize = CT_END + 12;
const TAG_SIZE: usize = 16; // ChaCha20Poly1305 auth tag

// the inner plaintext is a fixed block: two le bytes holding the real length, then payload, then zeros padding it out to INNER_SIZE
// encrypted whole, so the true payload length lives in the AEAD and never shows up as plaintext
// (plaintext length would let gpa read payload size despite uniform packet size).
const INNER_SIZE: usize = PACKET_SIZE - NONCE_END - TAG_SIZE;
const LEN_PREFIX: usize = 2;
const MAX_PAYLOAD: usize = INNER_SIZE - LEN_PREFIX;

const INSTRUCTION_VERSION: u8 = 1;

/// Instruction from Node A to a Decoy Source (Node B) telling it to emit cover traffic to a destination.
#[derive(Debug, Clone, Copy)]
pub struct DecoySourceInstruction {
    // Destination RDV node (Node C) TCP port.
    pub destination_addr: u16,
    // Number of noise packets to generate.
    pub packet_count: u32,
    // Byte length of the inner random-noise payload (not wire packet size;
    // every packet is padded and sealed to PACKET_SIZE). Controls only the
    // number of random bytes generated per packet, not the observable size.
    pub packet_size: u16,
    // Target bitrate in bits per second.
    pub bitrate_bps: u64,
}

/// Serialize a DecoySourceInstruction to a compact byte payload suitable for envelope::seal.
pub fn encode_instruction(instruction: &DecoySourceInstruction) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(17);
    buf.push(INSTRUCTION_VERSION);
    buf.extend_from_slice(&instruction.destination_addr.to_le_bytes());
    buf.extend_from_slice(&instruction.packet_count.to_le_bytes());
    buf.extend_from_slice(&instruction.packet_size.to_le_bytes());
    buf.extend_from_slice(&instruction.bitrate_bps.to_le_bytes());
    Ok(buf)
}

/// Parse a DecoySourceInstruction from bytes produced by encode_instruction.
pub fn decode_instruction(bytes: &[u8]) -> Result<DecoySourceInstruction> {
    if bytes.len() != 17 {
        bail!("instruction must be exactly 17 bytes, got {}", bytes.len());
    }
    let version = bytes[0];
    if version != INSTRUCTION_VERSION {
        bail!("unknown instruction version: {version}");
    }
    let destination_addr = u16::from_le_bytes([bytes[1], bytes[2]]);
    let packet_count = u32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]);
    let packet_size = u16::from_le_bytes([bytes[7], bytes[8]]);
    let bitrate_bps = u64::from_le_bytes([
        bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15], bytes[16],
    ]);
    if packet_count == 0 {
        bail!("packet_count must be > 0");
    }
    if packet_size == 0 || packet_size > MAX_PAYLOAD as u16 {
        bail!("packet_size must be 1..=MAX_PAYLOAD");
    }
    if bitrate_bps == 0 {
        bail!("bitrate_bps must be > 0");
    }
    Ok(DecoySourceInstruction {
        destination_addr,
        packet_count,
        packet_size,
        bitrate_bps,
    })
}

/// Encrypts payload with recipient's public key.
/// Adds prefix if needed so that ciphertext is always the same size
pub fn seal(recipient: &EncapsulationKey, payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() > MAX_PAYLOAD {
        bail!("payload too big for PACKET_SIZE");
    }
    let (kem_ct, shared_secret) = recipient.encapsulate();

    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).map_err(|error| anyhow::anyhow!("rng: {error}"))?;

    // fixed-size inner plaintext with real length prefixed inside it, so ciphertext is always the same size
    let mut inner = vec![0u8; INNER_SIZE];
    inner[..LEN_PREFIX].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    inner[LEN_PREFIX..LEN_PREFIX + payload.len()].copy_from_slice(payload);

    let aead = ChaCha20Poly1305::new(shared_secret.as_slice().into());
    let ciphertext = aead
        .encrypt(&nonce.into(), inner.as_slice())
        .map_err(|_| anyhow::anyhow!("aead seal failed"))?;

    let mut packet = Vec::with_capacity(PACKET_SIZE);
    packet.extend_from_slice(kem_ct.as_slice());
    packet.extend_from_slice(&nonce);
    packet.extend_from_slice(&ciphertext);
    debug_assert_eq!(packet.len(), PACKET_SIZE);
    Ok(packet)
}

/// try to open. wrong key -> kem gives a bogus shared secret -> aead auth fails
/// -> None, which the caller treats as "not for me, its decoy noise".
pub fn open(secret: &DecapsulationKey, packet: &[u8]) -> Option<Vec<u8>> {
    if packet.len() != PACKET_SIZE {
        return None;
    }

    let kem_ct = Ciphertext::try_from(&packet[..CT_END]).ok()?;
    let shared_secret = secret.decapsulate(&kem_ct);

    let nonce = &packet[CT_END..NONCE_END];
    let ciphertext = &packet[NONCE_END..];

    let aead = ChaCha20Poly1305::new(shared_secret.as_slice().into());
    let inner = aead.decrypt(nonce.into(), ciphertext).ok()?;

    // recover real length from inside the (authed) plaintext and strip zero padding
    if inner.len() != INNER_SIZE {
        return None;
    }
    let payload_len = u16::from_le_bytes([inner[0], inner[1]]) as usize;
    if payload_len > MAX_PAYLOAD {
        return None;
    }
    Some(inner[LEN_PREFIX..LEN_PREFIX + payload_len].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use x_wing::{Decapsulator, Kem, XWingKem};

    #[test]
    fn real_dest_opens_it() {
        let (secret, _) = XWingKem::generate_keypair();
        let packet = seal(secret.encapsulation_key(), b"hi").unwrap();
        assert_eq!(packet.len(), PACKET_SIZE); // fixed siez always
        assert_eq!(open(&secret, &packet).unwrap(), b"hi");
    }

    #[test]
    fn decoy_cant_open_it() {
        let (real, _) = XWingKem::generate_keypair();
        let (decoy, _) = XWingKem::generate_keypair();
        let packet = seal(real.encapsulation_key(), b"secret").unwrap();
        // decoy holds an unrelated key -> aead auth fail -> None
        assert!(open(&decoy, &packet).is_none());
    }

    #[test]
    fn two_seals_look_the_same_size() {
        let (real, _) = XWingKem::generate_keypair();
        let short = seal(real.encapsulation_key(), b"a").unwrap();
        let long = seal(real.encapsulation_key(), b"a much longer payload here").unwrap();
        assert_eq!(short.len(), long.len()); // wire size cant leak payload len
    }

    #[test]
    fn demonstrates_encode_decode_instruction_round_trip() {
        let instruction = DecoySourceInstruction {
            destination_addr: 4242,
            packet_count: 42,
            packet_size: 128,
            bitrate_bps: 500_000,
        };
        let encoded = encode_instruction(&instruction).unwrap();
        let decoded = decode_instruction(&encoded).unwrap();
        assert_eq!(decoded.destination_addr, instruction.destination_addr);
        assert_eq!(decoded.packet_count, instruction.packet_count);
        assert_eq!(decoded.packet_size, instruction.packet_size);
        assert_eq!(decoded.bitrate_bps, instruction.bitrate_bps);
    }

    #[test]
    fn demonstrates_decode_rejects_bad_input() {
        assert!(decode_instruction(&[]).is_err());
        assert!(decode_instruction(&[99]).is_err());
        // 17-byte inputs that pass the length check but hit each field validation branch.
        // Base valid fields: version=1, dest=1, count=5, size=100, bitrate=1
        //
        //  ─ version ─┬─ dest ─┬─ count ─┬─ size ─┬─ bitrate ─────────────
        // [1,          1,0,      5,0,0,0,   100,0,    1,0,0,0,0,0,0,0]
        assert!(
            decode_instruction(&[0, 1, 0, 5, 0, 0, 0, 100, 0, 1, 0, 0, 0, 0, 0, 0, 0,]).is_err()
        );
        //                       ↑ version=0 (expected 1)
        assert!(
            decode_instruction(&[1, 1, 0, 0, 0, 0, 0, 100, 0, 1, 0, 0, 0, 0, 0, 0, 0,]).is_err()
        );
        //                                ↑ packet_count=0
        assert!(decode_instruction(&[1, 1, 0, 5, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0,]).is_err());
        //                                            ↑ packet_size=0
        assert!(
            decode_instruction(&[1, 1, 0, 5, 0, 0, 0, 231, 3, 1, 0, 0, 0, 0, 0, 0, 0,]).is_err()
        );
        //                                            ↑ packet_size=999 > MAX_PAYLOAD(898)
        assert!(
            decode_instruction(&[1, 1, 0, 5, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0,]).is_err()
        );
        //                                                    ↑ bitrate_bps=0
    }
}
