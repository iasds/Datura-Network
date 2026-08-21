use data_encoding::BASE32_NOPAD;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest as Sha2Digest, Sha256};
use sha3::{Digest as Sha3Digest, Sha3_256};

pub type NodeId = [u8; 32];

/// A simple container for a node's identity.
/// It keeps the private signing key, the public verification key, and the derived node ID together.
pub struct Identity {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
    pub node_id: NodeId,
}

impl Default for Identity {
    fn default() -> Self {
        // Create a fresh random seed for the signing key so each node gets its own identity.
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);

        let signing_key = SigningKey::from_bytes(&bytes);
        let verifying_key: VerifyingKey = signing_key.verifying_key();

        let mut hasher = Sha256::new();
        hasher.update(verifying_key.as_bytes());

        let hash = hasher.finalize();

        let mut node_id = [0u8; 32];
        node_id.copy_from_slice(&hash);

        Self {
            signing_key,
            verifying_key,
            node_id,
        }
    }
}

impl Identity {
    /// Create a new identity with randomly generated keys.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build an identity from a deterministic seed value.
    /// This is handy for tests or for creating predictable identities.
    pub fn init_with_value(value: u8) -> Self {
        let mut bytes = [0u8; 32];
        bytes[0] = value;

        let signing_key = SigningKey::from_bytes(&bytes);
        let verifying_key: VerifyingKey = signing_key.verifying_key();

        // The node ID is public key's hash.
        let mut hasher = Sha256::new();
        hasher.update(verifying_key.as_bytes());

        let hash = hasher.finalize();

        let mut node_id = [0u8; 32];
        node_id.copy_from_slice(&hash);

        Self {
            signing_key,
            verifying_key,
            node_id,
        }
    }

    /// Turn the public key into a .dn address.
    pub fn get_address(&self) -> String {
        let public_key_bytes = self.verifying_key.to_bytes();

        let mut hasher = Sha3_256::new();
        hasher.update(b".dn checksum");
        hasher.update(public_key_bytes);
        hasher.update([0x03]); // version
        let digest = hasher.finalize();
        let checksum = &digest[..2];

        let mut onion_bytes = Vec::with_capacity(35);
        onion_bytes.extend_from_slice(&public_key_bytes);
        onion_bytes.extend_from_slice(checksum);
        onion_bytes.push(0x03);

        BASE32_NOPAD.encode(&onion_bytes).to_lowercase()
    }

    /// Print an overview of the identity for debugging or inspection.
    pub fn print_info(&self) {
        println!("Signing Key: {:x?}", self.signing_key.to_bytes());
        println!("Public Key: {:x?}", self.verifying_key.as_bytes());
        println!("Node ID: {:x?}", self.node_id);
        println!("Node ID (hex string): {}", hex::encode(self.node_id));
        println!("Address: {}.dn", self.get_address());
    }
}
