/// Per-hop encryption for Datura circuit legs.
///
/// Handshake (CREATE/CREATED):
///   1. Client generates ephemeral X25519 keypair, sends pubkey in CREATE cell.
///   2. Hop generates ephemeral X25519 keypair, sends pubkey in CREATED cell.
///   3. Both sides derive shared secret via X25519 DH.
///   4. HKDF-SHA256 expands the DH output into two 32-byte keys:
///      - forward key  (client→hop direction)
///      - backward key (hop→client direction)
///
/// Relay encryption (RELAY cells):
///   ChaCha20 counter mode — no authentication tag, so payload stays exactly
///   PAYLOAD_LEN bytes after encryption (no size expansion).  Each call to
///   apply_fwd/apply_bwd advances the keystream counter by PAYLOAD_LEN bytes,
///   keeping both sides of the channel in sync across multiple cells.

use chacha20::{
    cipher::{KeyIvInit, StreamCipher},
    ChaCha20,
};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};

pub const PUBKEY_LEN: usize = 32;

/// Symmetric stream-cipher keys for one hop of the onion circuit.
pub struct StreamKeys {
    fwd: ChaCha20, // client→hop (relays decrypt; client encrypts)
    bwd: ChaCha20, // hop→client (relays encrypt; client decrypts)
}

impl StreamKeys {
    pub fn from_shared(shared: &SharedSecret) -> Self {
        let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
        let mut okm = [0u8; 64];
        hk.expand(b"datura-stream-v0", &mut okm).unwrap();
        let fwd = ChaCha20::new(okm[..32].into(), [0u8; 12].as_slice().into());
        let bwd = ChaCha20::new(okm[32..].into(), [0u8; 12].as_slice().into());
        Self { fwd, bwd }
    }

    /// XOR buf with the forward keystream (client encrypts; relay decrypts — same op).
    pub fn apply_fwd(&mut self, buf: &mut [u8]) {
        self.fwd.apply_keystream(buf);
    }

    /// XOR buf with the backward keystream (relay encrypts reply; client decrypts — same op).
    pub fn apply_bwd(&mut self, buf: &mut [u8]) {
        self.bwd.apply_keystream(buf);
    }
}

/// Generate an ephemeral X25519 keypair. Returns (secret, public_bytes).
pub fn gen_keypair() -> (EphemeralSecret, [u8; PUBKEY_LEN]) {
    let secret = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
    let pubkey: [u8; PUBKEY_LEN] = PublicKey::from(&secret).to_bytes();
    (secret, pubkey)
}

/// Complete DH from our secret + their pubkey bytes.
pub fn complete_dh(secret: EphemeralSecret, their_pubkey: &[u8; PUBKEY_LEN]) -> SharedSecret {
    secret.diffie_hellman(&PublicKey::from(*their_pubkey))
}
