use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest as Sha2Digest, Sha256};
use sha3::{Digest as Sha3Digest, Sha3_256};
use data_encoding::BASE32_NOPAD;
use hex;

pub type NodeId = [u8; 32];

pub struct Identity {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
    pub node_id: NodeId,
}

impl Default for Identity {
    fn default() -> Self {
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn init_with_value(value: u8) -> Self {
        let mut bytes = [0u8; 32];
        bytes[0] = value;

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

    pub fn get_address(&self) -> String {
        let pk = self.verifying_key.to_bytes();

        // checksum = first 2 bytes of SHA3-256(".dn checksum" || pubkey || version)
        let mut hasher = Sha3_256::new();
        hasher.update(b".dn checksum");
        hasher.update(&pk);
        hasher.update([0x03]); // version
        let digest = hasher.finalize();
        let checksum = &digest[..2];

        // onion address bytes = pubkey || checksum || version
        let mut onion_bytes = Vec::with_capacity(35);
        onion_bytes.extend_from_slice(&pk);
        onion_bytes.extend_from_slice(checksum);
        onion_bytes.push(0x03);

        // base32 encode → 56 chars
        BASE32_NOPAD.encode(&onion_bytes).to_lowercase()
    }

    pub fn print_info(&self) {
        println!("Signing Key: {:x?}", self.signing_key.to_bytes());
        println!("Public Key: {:x?}", self.verifying_key.as_bytes());
        println!("Node ID: {:x?}", self.node_id);
        println!("Node ID (hex string): {}", hex::encode(self.node_id));
        println!("Address: {}.dn", self.get_address());
    }
    
}