use data_encoding::BASE32_NOPAD;
use ed25519_dalek::VerifyingKey;
use sha3::{Digest, Sha3_256};

const VERSION: u8 = 0x03;
const CHECKSUM_PREFIX: &[u8] = b".dn checksum";

// Derive a .dn hidden service address from an Ed25519 verifying key.

// Wire format (35 bytes, then base32-encoded):
//   pubkey[32] || sha3_256(".dn checksum" || pubkey || version)[:2] || version[1]

// Produces a 56-character label, which is the same length as Tor v3 .onion addresses.
// The pubkey is fully recoverable from the address with no network lookup.
pub fn pubkey_to_address(verifying_key: &VerifyingKey) -> String {
    let public_key_bytes = verifying_key.as_bytes();
    let checksum = compute_checksum(public_key_bytes);

    let mut payload = [0u8; 35];
    payload[..32].copy_from_slice(public_key_bytes);
    payload[32..34].copy_from_slice(&checksum);
    payload[34] = VERSION;

    format!("{}.dn", BASE32_NOPAD.encode(&payload).to_lowercase())
}

// Extract and validate an Ed25519 verifying key from a .dn address.
// Checks: correct suffix, valid base32, correct decoded length,
// supported version byte, and matching checksum.
// Accepts leading/trailing whitespace and a case-insensitive `.dn` suffix.
pub fn address_to_pubkey(address: &str) -> Result<VerifyingKey, String> {
    let address = address.trim();

    // Accept .dn in any case (e.g. .DN, .Dn) for robustness.
    let inner = if address.to_ascii_lowercase().ends_with(".dn") {
        &address[..address.len() - 3]
    } else {
        return Err(format!("not a .dn address: `{address}`"));
    };

    let decoded = BASE32_NOPAD
        .decode(inner.to_uppercase().as_bytes())
        .map_err(|e| format!("base32 decode error: {e}"))?;

    if decoded.len() != 35 {
        return Err(format!("expected 35 decoded bytes, got {}", decoded.len()));
    }

    let pubkey_bytes: &[u8; 32] = decoded[..32].try_into().unwrap();
    let version = decoded[34];

    if version != VERSION {
        return Err(format!("unsupported version byte {version:#04x}"));
    }

    let expected = compute_checksum(pubkey_bytes);
    if decoded[32..34] != expected {
        return Err("checksum mismatch: address is corrupted or tampered with".into());
    }

    VerifyingKey::from_bytes(pubkey_bytes).map_err(|e| format!("invalid pubkey bytes: {e}"))
}

fn compute_checksum(pubkey_bytes: &[u8; 32]) -> [u8; 2] {
    let mut hasher = Sha3_256::new();
    hasher.update(CHECKSUM_PREFIX);
    hasher.update(pubkey_bytes);
    hasher.update([VERSION]);
    let hash = hasher.finalize();
    [hash[0], hash[1]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn roundtrip() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let addr = pubkey_to_address(&verifying_key);

        assert!(addr.ends_with(".dn"), "address must end in .dn");
        assert_eq!(addr.len(), 59, "56 chars + 3 for '.dn'");

        let recovered = address_to_pubkey(&addr).expect("roundtrip failed");
        assert_eq!(verifying_key.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn rejects_bad_checksum() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let mut addr = pubkey_to_address(&verifying_key);
        // Flip the last character before ".dn"
        let flip_idx = addr.len() - 4;
        let original_char = addr.chars().nth(flip_idx).unwrap();
        let flipped_char = if original_char == 'a' { 'b' } else { 'a' };
        addr.replace_range(flip_idx..=flip_idx, &flipped_char.to_string());

        assert!(address_to_pubkey(&addr).is_err());
    }

    #[test]
    fn accepts_uppercase_suffix() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let addr = pubkey_to_address(&verifying_key);
        // Replace ".dn" with ".DN": should still parse correctly.
        let upper = format!("{}DN", &addr[..addr.len() - 2]);
        let recovered = address_to_pubkey(&upper).expect("uppercase .DN should be accepted");
        assert_eq!(verifying_key.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn accepts_whitespace_padding() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let addr = pubkey_to_address(&verifying_key);
        let padded = format!("  {addr}  ");
        let recovered = address_to_pubkey(&padded).expect("whitespace-padded address should parse");
        assert_eq!(verifying_key.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn rejects_wrong_suffix() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let addr = pubkey_to_address(&verifying_key);
        let onion = addr.replace(".dn", ".onion");
        assert!(address_to_pubkey(&onion).is_err());
    }

    #[test]
    fn rejects_wrong_label_length() {
        // Too short; only 10 chars before .dn
        assert!(address_to_pubkey("tooshort.dn").is_err());
        // Too long; 57 chars before .dn
        let long = format!("{}a.dn", "a".repeat(57));
        assert!(address_to_pubkey(&long).is_err());
    }
}
