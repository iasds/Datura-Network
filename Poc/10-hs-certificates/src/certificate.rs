// Certificate struct, build_certificate (prove), and verify_certificate, a
// repeatable function any party can call (ZK proof stays attached to the certificate for
// independent client-side re-verification.)

use std::sync::LazyLock;

use ff::PrimeField;
use group::ff::Field;
use group::{prime::PrimeCurveAffine, GroupEncoding};
use pasta_curves::{pallas, EqAffine};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use halo2_proofs::{
    plonk::{self, ProvingKey, SingleVerifier},
    poly::commitment::Params,
    transcript::{Blake2bRead, Blake2bWrite, Challenge255},
};

use crate::circuit::{envelope_message, hs_hash, CertificateCircuit};
use crate::dlog::derive_pk;
use crate::schnorr::{self, sign};

use halo2_gadgets::poseidon::primitives::{ConstantLength, Hash as PoseidonHash, P128Pow5T3};

// Circuit degree (2^K rows). The certificate
// circuit fits comfortably in 2^12 rows (the range-check lookup table
// needs 2^10; the ECC/Poseidon gates fit in remainder). Lower K means
// faster proving and verification, so this is set as low as the circuit
// allows. Network would generate PARAMS/PK once and distribute them, 
// rather than every node regenerating them from scratch.
pub const K: u32 = 12;

static PARAMS: LazyLock<Params<EqAffine>> = LazyLock::new(|| Params::new(K));
static PK: LazyLock<ProvingKey<EqAffine>> = LazyLock::new(|| {
    let empty = CertificateCircuit {
        sk: halo2_proofs::circuit::Value::unknown(),
        r_point: halo2_proofs::circuit::Value::unknown(),
        s: halo2_proofs::circuit::Value::unknown(),
    };
    let vk = plonk::keygen_vk(&PARAMS, &empty).expect("keygen_vk");
    plonk::keygen_pk(&PARAMS, vk, &empty).expect("keygen_pk")
});

// The RDV node's own counter-signature over the certificate's public
// fields, proving to any later verifier that the node actually agreed to
// the grant.
// This needs no ZK: an RDV node's identity hash Poseidon(pk) is public --> it can reveal pk and sign plainly.

// verify_endorsement checks that the pk actually hashes to the certificate's rdv_node_hash, so the endorsement can only come from the node the grant names.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Endorsement {
    pub rdv_pk: [u8; 32],
    pub sig_r: [u8; 32],
    pub sig_s: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Certificate {
    pub hs_hash: [u8; 32],
    pub rdv_node_hash: [u8; 32],
    pub expires: u64,
    pub pow_challenge: u128,
    pub pow_solution: [u8; 24],
    pub proof: Vec<u8>,
    // Attached by the RDV node after it accepts; None while the certificate is in flight from Node B to Node A.
    #[serde(default)]
    pub endorsement: Option<Endorsement>,
}

pub(crate) fn u128_to_fp(v: u128) -> pallas::Base {
    let mut repr = [0u8; 32];
    repr[..16].copy_from_slice(&v.to_le_bytes());
    pallas::Base::from_repr(repr).unwrap()
}

// Converts an externally-supplied 32-byte hash into an Fp element.

// This only works for hashes that are already valid canonical Fp encodings, not arbitrary SHA-family hashes
// from the rest of the network's .dn addressing scheme.
pub(crate) fn hash_bytes_to_fp(bytes: [u8; 32]) -> pallas::Base {
    pallas::Base::from_repr(bytes).expect("hash bytes must be a canonical Fp encoding")
}

// Shared prove/verify plumbing over the one circuit this crate has. The
// certificate and the routing instruction (routing.rs) are the same statement
// shape: "the key behind instance[0] signed Poseidon(instance[1..4])"
// so both go through these, reusing the same params and proving key.
pub(crate) fn prove_signed_envelope(
    sk: pallas::Scalar,
    r_point: pallas::Affine,
    s: pallas::Scalar,
    instance: [pallas::Base; 4],
) -> Vec<u8> {
    let circuit = CertificateCircuit {
        sk: halo2_proofs::circuit::Value::known(sk),
        r_point: halo2_proofs::circuit::Value::known(r_point),
        s: halo2_proofs::circuit::Value::known(s),
    };
    let mut transcript = Blake2bWrite::<_, EqAffine, _>::init(vec![]);
    plonk::create_proof(
        &PARAMS,
        &PK,
        &[circuit],
        &[&[&instance]],
        &mut OsRng,
        &mut transcript,
    )
    .expect("create_proof");
    transcript.finalize()
}

pub(crate) fn verify_signed_envelope(proof: &[u8], instance: [pallas::Base; 4]) -> bool {
    let strategy = SingleVerifier::new(&PARAMS);
    let mut transcript = Blake2bRead::<_, _, Challenge255<_>>::init(proof);
    plonk::verify_proof(&PARAMS, PK.get_vk(), strategy, &[&[&instance]], &mut transcript).is_ok()
}

// Builds a certificate: Node B (the hidden service destination), w/ sk,
// authorizes rdv_node_hash to route its traffic until expires, having been
// paid via the given (already-solved) PoW challenge/solution.
pub fn build_certificate(
    sk: pallas::Scalar,
    rdv_node_hash: [u8; 32],
    expires: u64,
    pow_challenge: u128,
    pow_solution: [u8; 24],
) -> Certificate {
    let pk = derive_pk(sk);
    let hs_hash_fp = hs_hash(pk);
    let rdv_hash_fp = hash_bytes_to_fp(rdv_node_hash);
    let expires_fp = pallas::Base::from(expires);
    let challenge_fp = u128_to_fp(pow_challenge);

    let m = envelope_message(rdv_hash_fp, expires_fp, challenge_fp);
    let k = pallas::Scalar::random(OsRng);
    let (r_point, s) = sign(sk, k, m);

    let instance = [hs_hash_fp, rdv_hash_fp, expires_fp, challenge_fp];
    let proof = prove_signed_envelope(sk, r_point, s, instance);

    Certificate {
        hs_hash: hs_hash_fp.to_repr(),
        rdv_node_hash,
        expires,
        pow_challenge,
        pow_solution,
        proof,
        endorsement: None,
    }
}

fn endorsement_digest(cert: &Certificate) -> Option<pallas::Base> {
    let hs = pallas::Base::from_repr(cert.hs_hash).into_option()?;
    let rdv = pallas::Base::from_repr(cert.rdv_node_hash).into_option()?;
    Some(
        PoseidonHash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init().hash([
            hs,
            rdv,
            pallas::Base::from(cert.expires),
            u128_to_fp(cert.pow_challenge),
        ]),
    )
}

// Node A (the RDV node) counter-signs an accepted certificate with its own
// identity key, committing publicly to the agreement.
pub fn endorse_certificate(sk_a: pallas::Scalar, cert: &Certificate) -> Endorsement {
    let m = endorsement_digest(cert).expect("accepted certificate has canonical field encodings");
    let k = pallas::Scalar::random(OsRng);
    let (r_point, s) = sign(sk_a, k, m);
    Endorsement {
        rdv_pk: derive_pk(sk_a).to_bytes(),
        sig_r: r_point.to_bytes(),
        sig_s: s.to_repr(),
    }
}

// Stateless endorsement check, callable by anyone holding the certificate:
// the revealed rdv_pk must hash to the certificate's rdv_node_hash, and the
// signature over the certificate's public fields must verify under it.
pub fn verify_endorsement(cert: &Certificate) -> bool {
    let Some(end) = &cert.endorsement else {
        return false;
    };
    let Some(pk_a) = pallas::Affine::from_bytes(&end.rdv_pk).into_option() else {
        return false;
    };
    if bool::from(pk_a.is_identity()) {
        return false;
    }
    if hs_hash(pk_a).to_repr() != cert.rdv_node_hash {
        return false;
    }
    let Some(r_point) = pallas::Affine::from_bytes(&end.sig_r).into_option() else {
        return false;
    };
    let Some(s) = pallas::Scalar::from_repr(end.sig_s).into_option() else {
        return false;
    };
    let Some(m) = endorsement_digest(cert) else {
        return false;
    };
    schnorr::verify(pk_a, r_point, s, m)
}

// Verifies a certificate. rebuilds the public-instance
// vector purely from cert's own fields and checks the attached proof against
// it. No private inputs, no dependency on how or when the certificate was
// issued, callable identically by the RDV node granting it or by any client
// checking it much later.

// Does not check PoW validity or expiry. See accept_certificate in main.rs
// for the full acceptance check, which also verifies the Equi-X solution and
// current-time-vs-expires. This function only answers "is the ZK proof valid
// for these exact fields."
pub fn verify_certificate(cert: &Certificate) -> bool {
    let hs_hash_fp = match pallas::Base::from_repr(cert.hs_hash).into_option() {
        Some(v) => v,
        None => return false,
    };
    let rdv_hash_fp = match pallas::Base::from_repr(cert.rdv_node_hash).into_option() {
        Some(v) => v,
        None => return false,
    };
    let expires_fp = pallas::Base::from(cert.expires);
    let challenge_fp = u128_to_fp(cert.pow_challenge);
    verify_signed_envelope(&cert.proof, [hs_hash_fp, rdv_hash_fp, expires_fp, challenge_fp])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_valid_certificate_verifies() {
        let sk = pallas::Scalar::random(OsRng);
        let rdv_node_hash: [u8; 32] = pallas::Base::random(OsRng).to_repr();
        let cert = build_certificate(sk, rdv_node_hash, 1_800_000_000, 42, [7u8; 24]);
        assert!(verify_certificate(&cert));
    }

    #[test]
    fn tampered_expires_fails_verification() {
        let sk = pallas::Scalar::random(OsRng);
        let rdv_node_hash: [u8; 32] = pallas::Base::random(OsRng).to_repr();
        let mut cert = build_certificate(sk, rdv_node_hash, 1_800_000_000, 42, [7u8; 24]);
        cert.expires = 9_999_999_999;
        assert!(!verify_certificate(&cert));
    }

    #[test]
    fn claimed_hash_not_matching_key_fails_verification() {
        let sk = pallas::Scalar::random(OsRng);
        let rdv_node_hash: [u8; 32] = pallas::Base::random(OsRng).to_repr();
        let mut cert = build_certificate(sk, rdv_node_hash, 1_800_000_000, 42, [7u8; 24]);
        cert.hs_hash = pallas::Base::random(OsRng).to_repr();
        assert!(!verify_certificate(&cert));
    }

    #[test]
    fn resolver_relaying_forged_certificate_fails() {
        // A malicious resolver that never received a real certificate cant
        // fabricate a valid proof by just inventing field values.
        let forged = Certificate {
            hs_hash: pallas::Base::random(OsRng).to_repr(),
            rdv_node_hash: pallas::Base::random(OsRng).to_repr(),
            expires: 1_800_000_000,
            pow_challenge: 42,
            pow_solution: [0u8; 24],
            proof: vec![0u8; 64],
            endorsement: None,
        };
        assert!(!verify_certificate(&forged));
    }

    #[test]
    fn endorsement_round_trip_verifies() {
        let sk_b = pallas::Scalar::random(OsRng);
        let sk_a = pallas::Scalar::random(OsRng);
        let node_a_hash: [u8; 32] = hs_hash(derive_pk(sk_a)).to_repr();
        let mut cert = build_certificate(sk_b, node_a_hash, 1_800_000_000, 42, [7u8; 24]);

        assert!(!verify_endorsement(&cert)); // not endorsed yet
        cert.endorsement = Some(endorse_certificate(sk_a, &cert));
        assert!(verify_endorsement(&cert));
    }

    #[test]
    fn endorsement_from_wrong_node_rejected() {
        // A node other than the one the certificate names cannot produce a
        // valid endorsement: its pk doesn't hash to rdv_node_hash.
        let sk_b = pallas::Scalar::random(OsRng);
        let sk_a = pallas::Scalar::random(OsRng);
        let sk_imposter = pallas::Scalar::random(OsRng);
        let node_a_hash: [u8; 32] = hs_hash(derive_pk(sk_a)).to_repr();
        let mut cert = build_certificate(sk_b, node_a_hash, 1_800_000_000, 42, [7u8; 24]);

        cert.endorsement = Some(endorse_certificate(sk_imposter, &cert));
        assert!(!verify_endorsement(&cert));
    }

    #[test]
    fn endorsement_over_tampered_fields_rejected() {
        // The endorsement digest binds every public field; changing any of
        // them after Node A signed must invalidate the endorsement.
        let sk_b = pallas::Scalar::random(OsRng);
        let sk_a = pallas::Scalar::random(OsRng);
        let node_a_hash: [u8; 32] = hs_hash(derive_pk(sk_a)).to_repr();
        let mut cert = build_certificate(sk_b, node_a_hash, 1_800_000_000, 42, [7u8; 24]);
        cert.endorsement = Some(endorse_certificate(sk_a, &cert));

        cert.expires = 9_999_999_999;
        assert!(!verify_endorsement(&cert));
    }
}
