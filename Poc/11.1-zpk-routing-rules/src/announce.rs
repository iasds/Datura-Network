use ff::PrimeField;
use group::{GroupEncoding, prime::PrimeCurveAffine};
use pasta_curves::pallas;

use crate::certificate::u128_to_fp;
use crate::circuit::hs_hash;
use crate::dlog::derive_pk;
use crate::schnorr::{self, sign};

use halo2_gadgets::poseidon::primitives::{ConstantLength, Hash as PoseidonHash, P128Pow5T3};

// Domain separator for the announcement digest
//
// The digest is 4-ary, which separates it from every other Poseidon use
// in this lineage: 2-ary hs_hash, 3-ary envelope_message (both
// the certificate and routing-instruction envelopes) and the 5-ary endorsement
// digest.
pub fn announce_domain() -> pallas::Base {
    pallas::Base::from_raw([0x636e756f6e6e61, 0, 0, 2])
}

// [pallas_pk: 32][port: 2 LE][pow_solution: 24][sig_r: 32][sig_s: 32]
pub const ANNOUNCE_LEN: usize = 122;

#[derive(Clone, Debug)]
pub struct Announce {
    pub pallas_pk: [u8; 32],
    pub port: u16,
    pub pow_solution: [u8; 24],
    pub sig_r: [u8; 32],
    pub sig_s: [u8; 32],
}

// The message an announcing node signs. Binds the hash being claimed, the port
// being claimed for it, and the challenge that makes the signature single-use.
pub fn announce_digest(node_hash: pallas::Base, port: u16, challenge: u128) -> pallas::Base {
    PoseidonHash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init().hash([
        node_hash,
        pallas::Base::from(port as u64),
        u128_to_fp(challenge),
        announce_domain(),
    ])
}

// Node C announces itself. The PoW solution is passed in already solved, so
// this function stays free of the solving step
pub fn build_announce(
    sk: pallas::Scalar,
    port: u16,
    challenge: u128,
    pow_solution: [u8; 24],
    k: pallas::Scalar,
) -> Announce {
    let pk = derive_pk(sk);
    let node_hash = hs_hash(pk);
    let (r_point, s) = sign(sk, k, announce_digest(node_hash, port, challenge));
    Announce {
        pallas_pk: pk.to_bytes(),
        port,
        pow_solution,
        sig_r: r_point.to_bytes(),
        sig_s: s.to_repr(),
    }
}

// Signature-only check, mirroring verify_certificate
pub fn verify_announce(ann: &Announce, challenge: u128) -> Option<[u8; 32]> {
    let pk = pallas::Affine::from_bytes(&ann.pallas_pk).into_option()?;
    // hs_hash unwraps affine coordinates, which the identity point does not
    // have, so this guard has to come first (as in verify_endorsement).
    if bool::from(pk.is_identity()) {
        return None;
    }
    let node_hash = hs_hash(pk);
    let r_point = pallas::Affine::from_bytes(&ann.sig_r).into_option()?;
    let s = pallas::Scalar::from_repr(ann.sig_s).into_option()?;

    if !schnorr::verify(
        pk,
        r_point,
        s,
        announce_digest(node_hash, ann.port, challenge),
    ) {
        return None;
    }
    Some(node_hash.to_repr())
}

impl Announce {
    pub fn to_bytes(&self) -> [u8; ANNOUNCE_LEN] {
        let mut out = [0u8; ANNOUNCE_LEN];
        out[..32].copy_from_slice(&self.pallas_pk);
        out[32..34].copy_from_slice(&self.port.to_le_bytes());
        out[34..58].copy_from_slice(&self.pow_solution);
        out[58..90].copy_from_slice(&self.sig_r);
        out[90..122].copy_from_slice(&self.sig_s);
        out
    }

    pub fn from_bytes(b: &[u8; ANNOUNCE_LEN]) -> Announce {
        let mut pallas_pk = [0u8; 32];
        let mut pow_solution = [0u8; 24];
        let mut sig_r = [0u8; 32];
        let mut sig_s = [0u8; 32];
        pallas_pk.copy_from_slice(&b[..32]);
        pow_solution.copy_from_slice(&b[34..58]);
        sig_r.copy_from_slice(&b[58..90]);
        sig_s.copy_from_slice(&b[90..122]);
        Announce {
            pallas_pk,
            port: u16::from_le_bytes([b[32], b[33]]),
            pow_solution,
            sig_r,
            sig_s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::route_domain;
    use group::ff::Field;
    use rand::rngs::OsRng;

    fn announce_for(sk: pallas::Scalar, port: u16, challenge: u128) -> Announce {
        build_announce(
            sk,
            port,
            challenge,
            [7u8; 24],
            pallas::Scalar::random(OsRng),
        )
    }

    #[test]
    fn round_trip_valid_announce_verifies() {
        let sk = pallas::Scalar::random(OsRng);
        let ann = announce_for(sk, 9131, 42);
        let expected = hs_hash(derive_pk(sk)).to_repr();
        assert_eq!(verify_announce(&ann, 42), Some(expected));
    }

    #[test]
    fn tampered_port_rejected() {
        // Redirecting a node's traffic to a different port on the same host by
        // editing the announcement in flight must break the signature.
        let sk = pallas::Scalar::random(OsRng);
        let mut ann = announce_for(sk, 9131, 42);
        ann.port = 9999;
        assert!(verify_announce(&ann, 42).is_none());
    }

    #[test]
    fn replay_against_a_different_challenge_rejected() {
        let sk = pallas::Scalar::random(OsRng);
        let ann = announce_for(sk, 9131, 42);
        assert!(verify_announce(&ann, 43).is_none());
    }

    #[test]
    fn signature_from_a_different_key_rejected() {
        // Claiming another node's hash requires its key: the hash is derived
        // from the revealed pk, so substituting a pk changes the hash the
        // signature was made over.
        let sk = pallas::Scalar::random(OsRng);
        let imposter = pallas::Scalar::random(OsRng);
        let mut ann = announce_for(sk, 9131, 42);
        ann.pallas_pk = derive_pk(imposter).to_bytes();
        assert!(verify_announce(&ann, 42).is_none());
    }

    #[test]
    fn identity_point_rejected() {
        // hs_hash unwraps affine coordinates, so an identity pk would panic
        // rather than fail if the guard were removed.
        let sk = pallas::Scalar::random(OsRng);
        let mut ann = announce_for(sk, 9131, 42);
        ann.pallas_pk = pallas::Affine::identity().to_bytes();
        assert!(verify_announce(&ann, 42).is_none());
    }

    #[test]
    fn malformed_encodings_rejected_without_panicking() {
        let sk = pallas::Scalar::random(OsRng);
        let mut ann = announce_for(sk, 9131, 42);
        let good = ann.clone();

        ann.pallas_pk = [0xff; 32];
        assert!(verify_announce(&ann, 42).is_none());

        ann = good.clone();
        ann.sig_r = [0xff; 32];
        assert!(verify_announce(&ann, 42).is_none());

        ann = good;
        ann.sig_s = [0xff; 32];
        assert!(verify_announce(&ann, 42).is_none());
    }

    #[test]
    fn announce_domain_is_distinct_from_route_domain() {
        // Both sit above the u128 range so neither can collide with a
        // pow_challenge, and they must not collide with each other either.
        assert_ne!(announce_domain(), route_domain());
        assert_eq!(announce_domain().to_repr()[24], 2);
        assert_eq!(route_domain().to_repr()[24], 1);
    }

    #[test]
    fn wire_round_trip_preserves_every_field() {
        let sk = pallas::Scalar::random(OsRng);
        let ann = announce_for(sk, 9131, 42);
        let back = Announce::from_bytes(&ann.to_bytes());
        assert_eq!(back.pallas_pk, ann.pallas_pk);
        assert_eq!(back.port, ann.port);
        assert_eq!(back.pow_solution, ann.pow_solution);
        assert_eq!(back.sig_r, ann.sig_r);
        assert_eq!(back.sig_s, ann.sig_s);
        assert!(verify_announce(&back, 42).is_some());
    }
}
