use ff::PrimeField;
use group::ff::Field;
use pasta_curves::pallas;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::certificate::{hash_bytes_to_fp, prove_signed_envelope, verify_signed_envelope};
use crate::circuit::{envelope_message, hs_hash};
use crate::dlog::derive_pk;
use crate::schnorr::sign;

// Domain separator occupying the instance slot that holds pow_challenge in a
// certificate. Its value is 2^192 + ascii("route_1"), chosen to be far above
// 2^128: a certificate's slot-3 value is u128_to_fp(pow_challenge) and can
// never reach it, so no valid certificate instance vector equals any routing
// instruction instance vector (and vice versa). That makes the two proof
// types structurally non-replayable against each other even though they share
// a circuit and the 3-ary Poseidon envelope.
pub fn route_domain() -> pallas::Base {
    pallas::Base::from_raw([0x315f6574756f72, 0, 0, 1])
}

// The instruction is deliberately proof-shaped like the certificate, but it
// is a private artifact: Node A verifies it once and keeps only
// (hs_hash --> target_node_hash) in its local table.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutingInstruction {
    pub hs_hash: [u8; 32],
    pub rdv_node_hash: [u8; 32],
    pub target_node_hash: [u8; 32],
    pub proof: Vec<u8>,
}

// Node B builds the instruction with the SAME sk that built the certificate;
// the resulting hs_hash therefore matches the certificate's, which is what
// lets Node A tie the two together without ever learning the key.
pub fn build_routing_instruction(
    sk: pallas::Scalar,
    rdv_node_hash: [u8; 32],
    target_node_hash: [u8; 32],
) -> RoutingInstruction {
    let hs_hash_fp = hs_hash(derive_pk(sk));
    let rdv_fp = hash_bytes_to_fp(rdv_node_hash);
    let target_fp = hash_bytes_to_fp(target_node_hash);

    let m = envelope_message(rdv_fp, target_fp, route_domain());
    let k = pallas::Scalar::random(OsRng);
    let (r_point, s) = sign(sk, k, m);

    let proof = prove_signed_envelope(sk, r_point, s, [hs_hash_fp, rdv_fp, target_fp, route_domain()]);

    RoutingInstruction {
        hs_hash: hs_hash_fp.to_repr(),
        rdv_node_hash,
        target_node_hash,
        proof,
    }
}

// Proof-only check, mirroring verify_certificate: rebuilds the instance from
// the instruction's own fields and checks the attached proof. Node A's full
// acceptance logic (hash-match against the endorsed certificate and against
// its own identity) lives in main.rs.
pub fn verify_routing_instruction(instr: &RoutingInstruction) -> bool {
    let hs_fp = match pallas::Base::from_repr(instr.hs_hash).into_option() {
        Some(v) => v,
        None => return false,
    };
    let rdv_fp = match pallas::Base::from_repr(instr.rdv_node_hash).into_option() {
        Some(v) => v,
        None => return false,
    };
    let target_fp = match pallas::Base::from_repr(instr.target_node_hash).into_option() {
        Some(v) => v,
        None => return false,
    };
    verify_signed_envelope(&instr.proof, [hs_fp, rdv_fp, target_fp, route_domain()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certificate::{build_certificate, u128_to_fp, verify_signed_envelope, Certificate};

    fn random_hash() -> [u8; 32] {
        hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr()
    }

    #[test]
    fn route_domain_exceeds_u128_range() {
        // The whole cross-replay argument rests on this: no u128 pow_challenge
        // can encode to the domain constant's field element.
        let max_challenge = u128_to_fp(u128::MAX);
        let domain_repr = route_domain().to_repr();
        // u128_to_fp writes only the low 16 bytes; the domain sets byte 24.
        assert_ne!(route_domain(), max_challenge);
        assert_eq!(domain_repr[24], 1);
    }

    #[test]
    fn round_trip_valid_instruction_verifies() {
        let sk = pallas::Scalar::random(OsRng);
        let instr = build_routing_instruction(sk, random_hash(), random_hash());
        assert!(verify_routing_instruction(&instr));
    }

    #[test]
    fn tampered_target_rejected() {
        // The RDV node (or anyone intercepting) redirecting the route to a
        // node of their choosing after signing must break the proof.
        let sk = pallas::Scalar::random(OsRng);
        let mut instr = build_routing_instruction(sk, random_hash(), random_hash());
        instr.target_node_hash = random_hash();
        assert!(!verify_routing_instruction(&instr));
    }

    #[test]
    fn claimed_hash_not_matching_key_rejected() {
        // Signing an instruction for a hidden service hash you don't control.
        let sk = pallas::Scalar::random(OsRng);
        let mut instr = build_routing_instruction(sk, random_hash(), random_hash());
        instr.hs_hash = random_hash();
        assert!(!verify_routing_instruction(&instr));
    }

    #[test]
    fn certificate_proof_not_replayable_as_instruction() {
        // A certificate's proof binds slot 3 to a u128 pow_challenge; the
        // instruction instance puts ROUTE_DOMAIN there, so re-wrapping a
        // certificate's proof as an instruction must fail even when the other three fields are copied over.
        let sk = pallas::Scalar::random(OsRng);
        let rdv = random_hash();
        let cert = build_certificate(sk, rdv, 1_800_000_000, 42, [7u8; 24]);

        let forged = RoutingInstruction {
            hs_hash: cert.hs_hash,
            rdv_node_hash: cert.rdv_node_hash,
            // Reuse the expires slot value as the "target" to line the
            // instances up as closely as a replay attacker could.
            target_node_hash: pallas::Base::from(cert.expires).to_repr(),
            proof: cert.proof,
        };
        assert!(!verify_routing_instruction(&forged));
    }

    #[test]
    fn instruction_proof_not_replayable_as_certificate() {
        let sk = pallas::Scalar::random(OsRng);
        let rdv = random_hash();
        let target = random_hash();
        let instr = build_routing_instruction(sk, rdv, target);

        // Try to pass the instruction's proof off as a certificate whose expires slot carries the target. 
        // verify_signed_envelope with the certificate-shaped instance must reject: slot 3 differs (u128
        // challenge vs ROUTE_DOMAIN) for every possible challenge value.
        let hs_fp = pallas::Base::from_repr(instr.hs_hash).unwrap();
        let rdv_fp = pallas::Base::from_repr(rdv).unwrap();
        let target_fp = pallas::Base::from_repr(target).unwrap();
        assert!(!verify_signed_envelope(
            &instr.proof,
            [hs_fp, rdv_fp, target_fp, u128_to_fp(42)]
        ));

        // And the JSON path: a Certificate carrying the instruction's proof.
        let forged = Certificate {
            hs_hash: instr.hs_hash,
            rdv_node_hash: rdv,
            expires: 1_800_000_000,
            pow_challenge: 42,
            pow_solution: [0u8; 24],
            proof: instr.proof,
            endorsement: None,
        };
        assert!(!crate::certificate::verify_certificate(&forged));
    }
}
