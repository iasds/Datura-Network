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

// seal payload to `recipient`. every sealed pkt is the exact same size & don't leak payload length, 8 dest's look identical
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

// try to open. wrong key -> kem gives a bogus shared secret -> aead auth fails
// -> None, which the caller treats as "not for me, its decoy noise".
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
}
