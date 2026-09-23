// Dual-keypair network identity.
//
// Every hidden service / node holds two keypairs:
//   - Ed25519: the .dn address, plain signatures, E2EE.
//   - pallas:  the ZK certificates, whose canonical network hash is Poseidon(pallas_pk.x, pallas_pk.y).
//
// The two keys are fused by two-way cross-signature: Ed25519 key signs
// the pallas public key, and pallas key (Schnorr) signs the Ed25519 pub key.
// Holding a verifying pair of cross-signatures proves both keys belong to
// the same principal, letting a client resolve a .dn address to the
// network's canonical pallas hash without trusting binding server.
//
// The cross-signatures are not part of a Certificate. The
// Ed25519 public key is the .dn address, so shipping it in the artifact the
// rdv receives would hand the rdv the identity. The binding travels
// separately, to those that already have the address.
//
// This hides the address from a node that does not already know it. It does not
// hide it from one that does! the address --> hash direction is open to anyone
// with a binding, so a node with a target address can compute that hash and
// blocklist!

use blake2::{Blake2b512, Digest as _};
use ff::{FromUniformBytes, PrimeField};
use group::ff::Field;
use group::{GroupEncoding, prime::PrimeCurveAffine};
use pasta_curves::pallas;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

use crate::address::{address_to_pubkey, pubkey_to_address};
use crate::circuit::hs_hash;
use crate::dlog::derive_pk;
use crate::schnorr;

// Domain-separation tags, one per cross-signature direction, so a signature
// for one direction doesn't work for other. The two directions
// already use different signature schemes, so the tags are redundant.
// they keep the separation if either side ever moves to the same curve.
//
// Separation from the pallas key's other signatures (certificate and routing
// envelopes) comes from construction not tags: those
// sign a poseidon output, while the cross-signature signs a
// Blake2b.
pub const CROSS_ED_CONTEXT: &[u8] = b"datura-cross-sig-ed-v1";
pub const CROSS_PALLAS_CONTEXT: &[u8] = b"datura-cross-sig-pallas-v1";

// Map arbitrary bytes to a pallas base-field element via a wide (512-bit)
// Blake2b digest (reduced uniformly). Unlike certificate::hash_bytes_to_fp, this
// accepts any input (an Ed25519 public key expected) not a canonical Fp
// encoding.
pub fn digest_to_fp(data: &[u8]) -> pallas::Base {
    let mut h = Blake2b512::new();
    h.update(data);
    let out = h.finalize();
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&out);
    pallas::Base::from_uniform_bytes(&wide)
}

// The full secret identity. Lives only in memory on the hs
// not to be sent anywhere !
pub struct IdentityKeys {
    pub ed_sk: SigningKey,
    pub pallas_sk: pallas::Scalar,
}

// public proof that the Ed25519 and pallas keys are the
// same principal. Ed25519 sigs are 64 bytes, so `ed_over_pallas` is a
// Vec (serde derives byte-array (de)serialization only up to length 32).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrossSignatures {
    pub ed_over_pallas: Vec<u8>, // Ed25519 sig over CROSS_ED_CONTEXT || pallas_pk_bytes
    pub pallas_sig_r: [u8; 32],  // Schnorr R  (Affine::to_bytes)
    pub pallas_sig_s: [u8; 32],  // Schnorr s  (Scalar::to_repr)
}

// Everything someone that knows .dn address needs to learn
// that address's canonical network hash and check it for itself. Safe to send those with address;
// must not be handed to a rdv
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddressBinding {
    pub pallas_pk: [u8; 32],
    pub cross_sigs: CrossSignatures,
}

pub fn generate_identity() -> IdentityKeys {
    IdentityKeys {
        ed_sk: SigningKey::generate(&mut OsRng),
        pallas_sk: pallas::Scalar::random(OsRng),
    }
}

// Canonical network id for this principal: Poseidon(pallas_pk).
// The hs_hash a certificate commits to, and the hashring position
pub fn canonical_hash(id: &IdentityKeys) -> [u8; 32] {
    hs_hash(derive_pk(id.pallas_sk)).to_repr()
}

pub fn dn_address(id: &IdentityKeys) -> String {
    pubkey_to_address(&id.ed_sk.verifying_key())
}

// The message the pallas key signs; uniform-mapped digest of the Ed25519
// public key. Uniform mapping (not from_repr) because an Ed25519 public key is
// not a canonical pallas field element
pub fn cross_pallas_message(ed_pk: &VerifyingKey) -> pallas::Base {
    let mut data = Vec::with_capacity(CROSS_PALLAS_CONTEXT.len() + 32);
    data.extend_from_slice(CROSS_PALLAS_CONTEXT);
    data.extend_from_slice(ed_pk.as_bytes());
    digest_to_fp(&data)
}

pub fn cross_sign(id: &IdentityKeys) -> CrossSignatures {
    let pallas_pk = derive_pk(id.pallas_sk);
    let pallas_pk_bytes = pallas_pk.to_bytes();
    let ed_pk = id.ed_sk.verifying_key();

    // Ed25519 signs the pallas public key.
    let mut ed_msg = Vec::with_capacity(CROSS_ED_CONTEXT.len() + 32);
    ed_msg.extend_from_slice(CROSS_ED_CONTEXT);
    ed_msg.extend_from_slice(&pallas_pk_bytes);
    let ed_sig = id.ed_sk.sign(&ed_msg);

    // The pallas key (Schnorr) signs the Ed25519 public key.
    let m = cross_pallas_message(&ed_pk);
    let k = pallas::Scalar::random(OsRng);
    let (r_point, s) = schnorr::sign(id.pallas_sk, k, m);

    CrossSignatures {
        ed_over_pallas: ed_sig.to_bytes().to_vec(),
        pallas_sig_r: r_point.to_bytes(),
        pallas_sig_s: s.to_repr(),
    }
}

// The publishable half of an identity: the pallas key and cross-signatures
// binding it to the .dn address.
pub fn publish_binding(id: &IdentityKeys) -> AddressBinding {
    AddressBinding {
        pallas_pk: derive_pk(id.pallas_sk).to_bytes(),
        cross_sigs: cross_sign(id),
    }
}

// Verify both directions of the cross-signature. By anyone holding
// the Ed25519 verifying key (from .dn) and the
// pallas public key bytes. Returns true if both signatures check out,
// which binds the two keys as one principal
pub fn verify_cross_signatures(
    ed_pk: &VerifyingKey,
    pallas_pk_bytes: &[u8; 32],
    cs: &CrossSignatures,
) -> bool {
    // Ed25519 direction.
    let Ok(sig_bytes) = <[u8; 64]>::try_from(cs.ed_over_pallas.as_slice()) else {
        return false;
    };
    let ed_sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
    let mut ed_msg = Vec::with_capacity(CROSS_ED_CONTEXT.len() + 32);
    ed_msg.extend_from_slice(CROSS_ED_CONTEXT);
    ed_msg.extend_from_slice(pallas_pk_bytes);
    if ed_pk.verify_strict(&ed_msg, &ed_sig).is_err() {
        return false;
    }

    // pallas direction.
    let Some(pallas_pk) = pallas::Affine::from_bytes(pallas_pk_bytes).into_option() else {
        return false;
    };
    if bool::from(pallas_pk.is_identity()) {
        return false;
    }
    let Some(r_point) = pallas::Affine::from_bytes(&cs.pallas_sig_r).into_option() else {
        return false;
    };
    let Some(s) = pallas::Scalar::from_repr(cs.pallas_sig_s).into_option() else {
        return false;
    };
    let m = cross_pallas_message(ed_pk);
    schnorr::verify(pallas_pk, r_point, s, m)
}

// .dn address to canonical network hash, authenticated end to end.
//
// The lookup a client performs: it knows a .dn address, but every artifact on the network refers
// to the hidden service by Poseidon(pallas_pk). Because both cross-signature
// directions are checked here, a directory (or any other party) that serves a
// binding cannot substitute its own pallas key for the address's: it would have
// to forge an Ed25519 signature under a key it does not hold.
//
// The returned hash is what a caller compares against Certificate::hs_hash to
// decide whether a certificate it was handed actually belongs to the address
// it was asked about.
pub fn resolve_hs_hash(dn_addr: &str, binding: &AddressBinding) -> Result<[u8; 32], String> {
    let ed_pk = address_to_pubkey(dn_addr)?;
    if !verify_cross_signatures(&ed_pk, &binding.pallas_pk, &binding.cross_sigs) {
        return Err("cross-signatures invalid: binding does not belong to this address".into());
    }
    // from_bytes already succeeded inside verify_cross_signatures.
    let pallas_pk = pallas::Affine::from_bytes(&binding.pallas_pk)
        .into_option()
        .ok_or("malformed pallas public key")?;
    Ok(hs_hash(pallas_pk).to_repr())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_signatures_round_trip() {
        let id = generate_identity();
        let ed_pk = id.ed_sk.verifying_key();
        let pallas_pk_bytes = derive_pk(id.pallas_sk).to_bytes();
        let cs = cross_sign(&id);
        assert!(verify_cross_signatures(&ed_pk, &pallas_pk_bytes, &cs));
    }

    #[test]
    fn ed_signature_over_wrong_pallas_key_rejected() {
        // The cross-signature is bound to a specific pallas key; presenting it
        // alongside a different pallas key must fail the Ed25519 direction.
        let id = generate_identity();
        let ed_pk = id.ed_sk.verifying_key();
        let cs = cross_sign(&id);
        let other_pallas = derive_pk(pallas::Scalar::random(OsRng)).to_bytes();
        assert!(!verify_cross_signatures(&ed_pk, &other_pallas, &cs));
    }

    #[test]
    fn pallas_signature_over_wrong_ed_key_rejected() {
        // Verifying against a different Ed25519 key must fail the pallas
        // direction (the pallas key signed a digest of the real ed key).
        let id = generate_identity();
        let pallas_pk_bytes = derive_pk(id.pallas_sk).to_bytes();
        let cs = cross_sign(&id);
        let other_ed = SigningKey::generate(&mut OsRng).verifying_key();
        assert!(!verify_cross_signatures(&other_ed, &pallas_pk_bytes, &cs));
    }

    #[test]
    fn wrong_length_ed_signature_rejected() {
        let id = generate_identity();
        let ed_pk = id.ed_sk.verifying_key();
        let pallas_pk_bytes = derive_pk(id.pallas_sk).to_bytes();
        let mut cs = cross_sign(&id);
        cs.ed_over_pallas.truncate(63);
        assert!(!verify_cross_signatures(&ed_pk, &pallas_pk_bytes, &cs));
    }

    #[test]
    fn identity_point_pallas_key_rejected() {
        let id = generate_identity();
        let ed_pk = id.ed_sk.verifying_key();
        let cs = cross_sign(&id);
        let identity_bytes = pallas::Affine::identity().to_bytes();
        assert!(!verify_cross_signatures(&ed_pk, &identity_bytes, &cs));
    }

    #[test]
    fn resolve_matches_canonical_hash() {
        let id = generate_identity();
        let resolved = resolve_hs_hash(&dn_address(&id), &publish_binding(&id))
            .expect("binding from the same identity must resolve");
        assert_eq!(resolved, canonical_hash(&id));
    }

    #[test]
    fn resolve_rejects_binding_from_another_principal() {
        // A directory that serves someone else's pallas key alongside a victim's
        // address is the whole attack this binding exists to stop: the attacker
        // would otherwise steer clients at a hash it controls the key for.
        let victim = generate_identity();
        let attacker = generate_identity();
        let err = resolve_hs_hash(&dn_address(&victim), &publish_binding(&attacker))
            .expect_err("another principal's binding must not resolve");
        assert!(err.contains("cross-signatures invalid"));
    }

    #[test]
    fn resolve_rejects_spliced_binding() {
        // Splicing the victim's cross-signatures onto the attacker's pallas key
        // fails too: the Ed25519 direction is bound to the pallas key bytes.
        let victim = generate_identity();
        let attacker = generate_identity();
        let spliced = AddressBinding {
            pallas_pk: derive_pk(attacker.pallas_sk).to_bytes(),
            cross_sigs: cross_sign(&victim),
        };
        assert!(resolve_hs_hash(&dn_address(&victim), &spliced).is_err());
    }

    #[test]
    fn resolve_rejects_malformed_address() {
        let id = generate_identity();
        assert!(resolve_hs_hash("not-an-address", &publish_binding(&id)).is_err());
    }
}
