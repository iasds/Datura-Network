use ff::PrimeField;
use group::ff::Field;
use pasta_curves::pallas;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::certificate::{
    hash_bytes_to_fp, prove_signed_envelope, u128_to_fp, verify_signed_envelope,
};
use crate::circuit::{envelope_message, hs_hash, route_binding as route_binding_hash};
use crate::dlog::derive_pk;
use crate::schnorr::sign;

// Domain separator folded into the instance slot that holds pow_challenge in a
// certificate. Its value is 2^192 + ascii("route_1"), chosen to be far above
// 2^128: a certificate's slot-3 value is u128_to_fp(pow_challenge) and can
// never reach it, so no valid certificate instance vector equals any routing
// instruction instance vector (and vice versa). That makes the two proof
// types structurally non-replayable against each other even though they share
// a circuit and the 3-ary Poseidon envelope.
pub fn route_domain() -> pallas::Base {
    pallas::Base::from_raw([0x315f6574756f72, 0, 0, 1])
}

// Node B's connection entropy, mixed into the instruction's binding alongside the challenge
pub const CLIENT_NONCE_LEN: usize = 16;

// Fresh nonce for one instruction. B calls per connection
pub fn new_client_nonce() -> [u8; CLIENT_NONCE_LEN] {
    let mut nonce = [0u8; CLIENT_NONCE_LEN];
    getrandom::fill(&mut nonce).expect("system entropy unavailable");
    nonce
}

// Slot 3 of an instruction's instance: the domain separator bound to both the
// challenge Node A issued for this grant and the nonce Node B generated for
// this connection
pub fn route_binding(pow_challenge: u128, client_nonce: [u8; CLIENT_NONCE_LEN]) -> pallas::Base {
    // The nonce is 16 bytes, so it lands in the low half of the repr and is
    // always canonical, same as u128_to_fp
    let mut nonce_repr = [0u8; 32];
    nonce_repr[..CLIENT_NONCE_LEN].copy_from_slice(&client_nonce);
    let nonce_fp = pallas::Base::from_repr(nonce_repr).unwrap();

    let mixed = route_binding_hash(route_domain(), u128_to_fp(pow_challenge), nonce_fp).to_repr();
    let mut repr = [0u8; 32];
    repr[..24].copy_from_slice(&mixed[..24]);
    repr[24] = 1;
    pallas::Base::from_repr(repr).unwrap()
}

// The instruction is deliberately proof-shaped like the certificate, but it
// is a private artifact: Node A verifies it once and keeps only
// (hs_hash --> target_node_hash) in its local table.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutingInstruction {
    pub hs_hash: [u8; 32],
    pub rdv_node_hash: [u8; 32],
    pub target_node_hash: [u8; 32],
    // Node B's per-connection entropy. Node A needs it to rebuild slot 3.
    // not a secret: just to be unpredictable
    // to Node A before the instruction is built, and to be a value Node A has
    // to present after
    pub client_nonce: [u8; CLIENT_NONCE_LEN],
    pub proof: Vec<u8>,
}

// Node B builds the instruction with the SAME sk that built the certificate;
// the resulting hs_hash therefore matches the certificate's, which is what
// lets Node A tie the two together without ever learning the key.
//
// `pow_challenge` must be the challenge Node A issued for the grant this
// instruction accompanies, the same value the certificate has
//
// `client_nonce` must be freshly generated for this connection
//
// Returns None if either hash is not a canonical field element (always the
// case for real node/target hashes; this only rejects malformed input).
pub fn build_routing_instruction(
    sk: pallas::Scalar,
    rdv_node_hash: [u8; 32],
    target_node_hash: [u8; 32],
    pow_challenge: u128,
    client_nonce: [u8; CLIENT_NONCE_LEN],
) -> Option<RoutingInstruction> {
    let hs_hash_fp = hs_hash(derive_pk(sk));
    let rdv_fp = hash_bytes_to_fp(rdv_node_hash)?;
    let target_fp = hash_bytes_to_fp(target_node_hash)?;
    let binding = route_binding(pow_challenge, client_nonce);

    let m = envelope_message(rdv_fp, target_fp, binding);
    let k = pallas::Scalar::random(OsRng);
    let (r_point, s) = sign(sk, k, m);

    let proof = prove_signed_envelope(sk, r_point, s, [hs_hash_fp, rdv_fp, target_fp, binding]);

    Some(RoutingInstruction {
        hs_hash: hs_hash_fp.to_repr(),
        rdv_node_hash,
        target_node_hash,
        client_nonce,
        proof,
    })
}

// Proof-only check, mirroring verify_certificate: rebuilds the instance from
// the instruction's own fields and checks the attached proof. Node A's full
// acceptance logic (hash-match against the endorsed certificate and against
// its own identity) lives in main.rs.
//
// `pow_challenge` is the challenge Node A issued on this connection, an
// instruction built for any other ceremony rebuilds a different slot-3 value
//
// The nonce comes from the instruction, so this does not establish that
// it is fresh: tampering with it breaks the proof, but an attacker replaying a
// whole instruction replays its nonce too. Rejecting a nonce already seen this
// session is on Node A's
pub fn verify_routing_instruction(instr: &RoutingInstruction, pow_challenge: u128) -> bool {
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
    verify_signed_envelope(
        &instr.proof,
        [
            hs_fp,
            rdv_fp,
            target_fp,
            route_binding(pow_challenge, instr.client_nonce),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certificate::{Certificate, build_certificate, u128_to_fp, verify_signed_envelope};

    fn random_hash() -> [u8; 32] {
        hs_hash(derive_pk(pallas::Scalar::random(OsRng))).to_repr()
    }

    // A fixed nonce, for tests whose subject is not freshness
    const NONCE: [u8; CLIENT_NONCE_LEN] = [9u8; CLIENT_NONCE_LEN];

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
    fn route_binding_stays_out_of_u128_range() {
        // Same invariant, now for the value in slot 3: mixing
        // the challenge and the nonce in cant drag it into the range
        // certificates occupy
        for challenge in [0u128, 1, 42, u128::MAX] {
            for nonce in [[0u8; CLIENT_NONCE_LEN], NONCE, [0xffu8; CLIENT_NONCE_LEN]] {
                let repr = route_binding(challenge, nonce).to_repr();
                assert_eq!(repr[24], 1, "challenge {challenge} left slot 3 below 2^192");
                assert!(repr[25..].iter().all(|b| *b == 0));
                assert_ne!(route_binding(challenge, nonce), u128_to_fp(challenge));
            }
        }
    }

    #[test]
    fn route_binding_is_challenge_dependent() {
        // If distinct challenges collapse to one binding, the replay would work
        assert_ne!(route_binding(42, NONCE), route_binding(43, NONCE));
        assert_ne!(route_binding(0, NONCE), route_binding(u128::MAX, NONCE));
    }

    #[test]
    fn route_binding_is_nonce_dependent() {
        // The point of the nonce: with the challenge held fixed (what a
        // malicious Node A gets to do by reissuing one), a different nonce must
        // still land on a different slot-3 value.
        assert_ne!(
            route_binding(42, [1u8; CLIENT_NONCE_LEN]),
            route_binding(42, [2u8; CLIENT_NONCE_LEN])
        );
        // A one-bit change is enough; Poseidon is not a truncation of its input.
        let mut off_by_one = NONCE;
        off_by_one[0] ^= 1;
        assert_ne!(route_binding(42, NONCE), route_binding(42, off_by_one));
    }

    #[test]
    fn challenge_and_nonce_are_not_interchangeable() {
        // Slot 3 must not be a function of some combination the two inputs can
        // be traded off against; swapping which value carries what has to move it.
        let a = route_binding(1, [2u8; CLIENT_NONCE_LEN]);
        let b = route_binding(2, [1u8; CLIENT_NONCE_LEN]);
        assert_ne!(a, b);
    }

    #[test]
    fn new_client_nonce_does_not_repeat() {
        // Node B's freshness comes from generating, so the
        // generator must produce distinct values
        let a = new_client_nonce();
        let b = new_client_nonce();
        assert_ne!(a, b);
        assert_ne!(a, [0u8; CLIENT_NONCE_LEN]);
    }

    #[test]
    fn round_trip_valid_instruction_verifies() {
        let sk = pallas::Scalar::random(OsRng);
        let instr = build_routing_instruction(sk, random_hash(), random_hash(), 42, NONCE).unwrap();
        assert!(verify_routing_instruction(&instr, 42));
    }

    #[test]
    fn instruction_from_another_challenge_rejected() {
        // an instruction built for one grant ceremony can't verify under a different challenge
        let sk = pallas::Scalar::random(OsRng);
        let instr = build_routing_instruction(sk, random_hash(), random_hash(), 42, NONCE).unwrap();
        assert!(!verify_routing_instruction(&instr, 43));
    }

    #[test]
    fn instruction_from_another_nonce_rejected() {
        // The replay under a reissued challenge: the challenge matches (Node A
        // chose it twice), so only the nonce separates the two instructions.
        // Rebuilding the instance with a different nonce must fail
        let sk = pallas::Scalar::random(OsRng);
        let rdv = random_hash();
        let target = random_hash();
        let instr =
            build_routing_instruction(sk, rdv, target, 42, [1u8; CLIENT_NONCE_LEN]).unwrap();

        // Swapping the nonce does not make the proof verify: the
        // nonce it was proved under is the only one that works
        let mut tampered = instr.clone();
        tampered.client_nonce = [2u8; CLIENT_NONCE_LEN];
        assert!(!verify_routing_instruction(&tampered, 42));

        // And an instruction honestly built for the same route and challenge
        // under a different nonce is a different artifact.
        let other =
            build_routing_instruction(sk, rdv, target, 42, [2u8; CLIENT_NONCE_LEN]).unwrap();
        assert_ne!(other.proof, instr.proof);
        assert!(verify_routing_instruction(&other, 42));
    }

    #[test]
    fn tampered_target_rejected() {
        // The RDV node (or anyone intercepting) redirecting the route to a
        // node of their choosing after signing must break the proof.
        let sk = pallas::Scalar::random(OsRng);
        let mut instr =
            build_routing_instruction(sk, random_hash(), random_hash(), 42, NONCE).unwrap();
        instr.target_node_hash = random_hash();
        assert!(!verify_routing_instruction(&instr, 42));
    }

    #[test]
    fn claimed_hash_not_matching_key_rejected() {
        // Signing an instruction for a hidden service hash you don't control.
        let sk = pallas::Scalar::random(OsRng);
        let mut instr =
            build_routing_instruction(sk, random_hash(), random_hash(), 42, NONCE).unwrap();
        instr.hs_hash = random_hash();
        assert!(!verify_routing_instruction(&instr, 42));
    }

    #[test]
    fn certificate_proof_not_replayable_as_instruction() {
        // A certificate's proof binds slot 3 to a u128 pow_challenge; the
        // instruction instance puts route_binding(challenge) there, which is
        // always above 2^192, so re-wrapping a certificate's proof as an
        // instruction must fail even when the other three fields are copied
        // over and the challenge is used to rebuild the instance
        let sk = pallas::Scalar::random(OsRng);
        let rdv = random_hash();
        let cert = build_certificate(
            sk,
            rdv,
            1_800_000_000 - 86_400,
            1_800_000_000,
            42,
            [7u8; 24],
        )
        .unwrap();

        let forged = RoutingInstruction {
            hs_hash: cert.hs_hash,
            rdv_node_hash: cert.rdv_node_hash,
            // Reuse the expires slot value as the "target" to line the
            // instances up as closely as a replay attacker could.
            target_node_hash: pallas::Base::from(cert.expires).to_repr(),
            client_nonce: NONCE,
            proof: cert.proof,
        };
        assert!(!verify_routing_instruction(&forged, 42));
    }

    #[test]
    fn instruction_proof_not_replayable_as_certificate() {
        let sk = pallas::Scalar::random(OsRng);
        let rdv = random_hash();
        let target = random_hash();
        let instr = build_routing_instruction(sk, rdv, target, 42, NONCE).unwrap();

        // Try to pass the instruction's proof off as a certificate whose expires slot carries the target.
        // verify_signed_envelope with the certificate-shaped instance must reject: slot 3 differs (u128
        // challenge vs route_binding, which is >= 2^192) for every possible challenge value.
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
            issued_at: 1_800_000_000 - 86_400,
            expires: 1_800_000_000,
            pow_challenge: 42,
            pow_solution: [0u8; 24],
            proof: instr.proof,
            endorsement: None,
        };
        assert!(!crate::certificate::verify_certificate(&forged));
    }
}
