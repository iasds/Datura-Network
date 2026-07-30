use ed25519_dalek::{SigningKey, VerifyingKey};
use hpke_rs::{hpke_types::*, *};
use hpke_rs_libcrux::HpkeLibcrux;
use sha2::{Digest, Sha512};

// Application-specific info string bound into every HPKE context.
// Changing this is a breaking change as different info = different shared secret.

// Note: HPKE Base mode (no sender authentication) provides no replay protection.
// A recorded (kem_output, ciphertext) pair can be re-submitted to the server and
// will decrypt correctly. Adding a per-session nonce to the wire format and binding
// it into `info` is the correct fix;
// that is out of scope for this Poc.
const INFO: &[u8] = b"datura-network-poc9";

/// DhKem25519 encapsulated key is always 32 bytes.
/// Used to read exactly this many bytes from the wire without a length prefix.
pub const KEM_OUTPUT_LEN: usize = 32;

fn make_hpke() -> Hpke<HpkeLibcrux> {
    Hpke::new(
        Mode::Base,
        KemAlgorithm::DhKem25519,
        KdfAlgorithm::HkdfSha256,
        AeadAlgorithm::ChaCha20Poly1305,
    )
}

// Convert an Ed25519 verifying key to an HPKE public key.
// Ed25519 and X25519 share the same underlying Curve25519.
// `to_montgomery()` converts the Edwards point to its Montgomery (u-coordinate)
// representation, which is exactly the X25519 public key format.
pub fn ed25519_to_hpke_public(verifying_key: &VerifyingKey) -> HpkePublicKey {
    HpkePublicKey::new(verifying_key.to_montgomery().to_bytes().to_vec())
}

// Convert an Ed25519 signing key to an HPKE private key.
// X25519 requires SHA-512(seed)[0..32] with RFC 7748 §5 clamping applied.
// `to_scalar()` returns the scalar reduced mod l (group order), which is a
// different value — using it produces a public key mismatch with `to_montgomery()`.
pub fn ed25519_to_hpke_private(signing_key: &SigningKey) -> HpkePrivateKey {
    let hash = Sha512::digest(signing_key.to_bytes());
    let mut x25519_sk = [0u8; 32];
    x25519_sk.copy_from_slice(&hash[..32]);
    // RFC 7748 §5 clamping
    x25519_sk[0] &= 248;
    x25519_sk[31] &= 127;
    x25519_sk[31] |= 64;
    HpkePrivateKey::new(x25519_sk.to_vec())
}

// Encrypt `plaintext` for a recipient known only by their Ed25519 verifying key.
//
// Returns `(kem_output, ciphertext)`:
// - `kem_output` is always 32 bytes (ephemeral X25519 public key)
// - `ciphertext` is `plaintext.len() + 16` bytes (ChaCha20Poly1305 tag overhead)
pub fn encrypt(
    recipient_verifying_key: &VerifyingKey,
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), HpkeError> {
    let hpke_public_key = ed25519_to_hpke_public(recipient_verifying_key);
    let (kem_output, mut sender_context) =
        make_hpke().setup_sender(&hpke_public_key, INFO, None, None, None)?;
    let ciphertext = sender_context.seal(b"", plaintext)?;
    Ok((kem_output, ciphertext))
}

// Decrypt a message using the hidden service's Ed25519 signing key.
//
// `kem_output` must be the 32-byte value produced by `encrypt`.
// `ciphertext` must be the accompanying ciphertext bytes.
pub fn decrypt(
    signing_key: &SigningKey,
    kem_output: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, HpkeError> {
    let privkey = ed25519_to_hpke_private(signing_key);
    let mut receiver_context =
        make_hpke().setup_receiver(kem_output, &privkey, INFO, None, None, None)?;
    receiver_context.open(b"", ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let message = b"hello datura network!";
        let (kem_output, ciphertext) = encrypt(&verifying_key, message).expect("encrypt failed");

        assert_eq!(kem_output.len(), KEM_OUTPUT_LEN);
        assert_eq!(ciphertext.len(), message.len() + 16); // "ChaCha20Poly1305" tag

        let decrypted = decrypt(&signing_key, &kem_output, &ciphertext).expect("decrypt failed");
        assert_eq!(decrypted, message);
    }

    #[test]
    fn wrong_key_fails() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let wrong_signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let (kem_output, ciphertext) = encrypt(&verifying_key, b"secret").expect("encrypt failed");
        assert!(decrypt(&wrong_signing_key, &kem_output, &ciphertext).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let (kem_output, mut ciphertext) =
            encrypt(&verifying_key, b"secret").expect("encrypt failed");
        ciphertext[0] ^= 0xff;
        assert!(decrypt(&signing_key, &kem_output, &ciphertext).is_err());
    }

    #[test]
    fn empty_ciphertext_fails() {
        // A zero-length ciphertext can never contain a valid AEAD tag.
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let (kem_output, _) = encrypt(&verifying_key, b"x").expect("encrypt failed");
        assert!(decrypt(&signing_key, &kem_output, &[]).is_err());
    }

    #[test]
    fn short_ciphertext_fails() {
        // Anything under 16 bytes is too short to contain the Poly1305 tag.
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let (kem_output, _) = encrypt(&verifying_key, b"x").expect("encrypt failed");
        assert!(decrypt(&signing_key, &kem_output, &[0u8; 15]).is_err());
    }

    #[test]
    fn invalid_kem_output_length_fails() {
        // KEM output must be exactly KEM_OUTPUT_LEN bytes; wrong length should fail.
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let (_, ciphertext) = encrypt(&verifying_key, b"x").expect("encrypt failed");
        // Too short
        assert!(decrypt(&signing_key, &[0u8; 16], &ciphertext).is_err());
        // Too long
        assert!(decrypt(&signing_key, &[0u8; 64], &ciphertext).is_err());
    }

    #[test]
    fn encrypt_empty_plaintext() {
        // Empty plaintext produces a 16-byte ciphertext (just the AEAD tag).
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let (kem_output, ciphertext) = encrypt(&verifying_key, b"").expect("encrypt failed");
        assert_eq!(ciphertext.len(), 16);
        let decrypted = decrypt(&signing_key, &kem_output, &ciphertext).expect("decrypt failed");
        assert_eq!(decrypted, b"");
    }
}
