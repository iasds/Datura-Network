use pasta_curves::pallas;

use crate::dlog::{configure_ecc, derive_pk, load_plain_range_table, HsEccChip, HsEccConfig, HsGenerator};
use halo2_gadgets::ecc::{FixedPoint, NonIdentityPoint, ScalarFixed, ScalarVar};
use halo2_gadgets::poseidon::{
    primitives::{ConstantLength, P128Pow5T3},
    Hash, Pow5Chip, Pow5Config,
};
use halo2_proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Fixed, Instance, TableColumn},
};

const WIDTH: usize = 3;
const RATE: usize = 2;

#[derive(Clone)]
pub struct CertificateConfig {
    ecc: HsEccConfig,
    poseidon: Pow5Config<pallas::Base, WIDTH, RATE>,
    poseidon_state: [Column<Advice>; WIDTH],
    lookup_table: TableColumn,
    instance: Column<Instance>,
}

fn configure_poseidon(
    meta: &mut ConstraintSystem<pallas::Base>,
) -> (Pow5Config<pallas::Base, WIDTH, RATE>, [Column<Advice>; WIDTH]) {
    let poseidon_state: [Column<Advice>; WIDTH] = (0..WIDTH)
        .map(|_| meta.advice_column())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let partial_sbox = meta.advice_column();
    let rc_a: [Column<Fixed>; WIDTH] = (0..WIDTH).map(|_| meta.fixed_column()).collect::<Vec<_>>().try_into().unwrap();
    let rc_b: [Column<Fixed>; WIDTH] = (0..WIDTH).map(|_| meta.fixed_column()).collect::<Vec<_>>().try_into().unwrap();
    meta.enable_constant(rc_b[0]);
    for c in &poseidon_state {
        meta.enable_equality(*c);
    }
    let poseidon = Pow5Chip::configure::<P128Pow5T3>(meta, poseidon_state, partial_sbox, rc_a, rc_b);
    (poseidon, poseidon_state)
}

pub struct CertificateCircuit {
    pub sk: Value<pallas::Scalar>,
    pub r_point: Value<pallas::Affine>,
    pub s: Value<pallas::Scalar>,
}

impl Circuit<pallas::Base> for CertificateCircuit {
    type Config = CertificateConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self {
            sk: Value::unknown(),
            r_point: Value::unknown(),
            s: Value::unknown(),
        }
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> CertificateConfig {
        let (ecc, lookup_table) = configure_ecc(meta);
        let (poseidon, poseidon_state) = configure_poseidon(meta);
        let instance = meta.instance_column();
        meta.enable_equality(instance);

        CertificateConfig {
            ecc,
            poseidon,
            poseidon_state,
            lookup_table,
            instance,
        }
    }

    fn synthesize(
        &self,
        config: CertificateConfig,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        load_plain_range_table(config.lookup_table, &mut layouter)?;

        let ecc_chip = HsEccChip::construct(config.ecc, halo2_gadgets::ecc::chip::CircuitVersion::AnchoredBase);

        // Constraint 1: pk = [sk]G (witness + derive + constrain equal).
        let pk_value = self.sk.map(derive_pk);
        let pk = NonIdentityPoint::new(ecc_chip.clone(), layouter.namespace(|| "witness pk"), pk_value)?;
        let sk_scalar = ScalarFixed::new(ecc_chip.clone(), layouter.namespace(|| "sk"), self.sk)?;
        let generator = FixedPoint::from_inner(ecc_chip.clone(), HsGenerator);
        let (pk_derived, _) = generator.mul(layouter.namespace(|| "[sk]G"), sk_scalar)?;
        pk.constrain_equal(layouter.namespace(|| "pk == [sk]G"), &pk_derived)?;

        // three "envelope" public fields (rdv_node_hash, expires, pow_challenge) that the signed message binds together.
        let (rdv_hash_cell, expires_cell, challenge_cell) = layouter.assign_region(
            || "load envelope fields",
            |mut region| {
                let rdv_hash = region.assign_advice_from_instance(
                    || "rdv_node_hash",
                    config.instance,
                    1,
                    config.poseidon_state[0],
                    0,
                )?;
                let expires = region.assign_advice_from_instance(
                    || "expires",
                    config.instance,
                    2,
                    config.poseidon_state[1],
                    0,
                )?;
                let challenge = region.assign_advice_from_instance(
                    || "pow_challenge",
                    config.instance,
                    3,
                    config.poseidon_state[2],
                    0,
                )?;
                Ok((rdv_hash, expires, challenge))
            },
        )?;

        // Constraint 2: hs_hash == Poseidon(pk.x, pk.y).
        let h_computed = Hash::<_, _, P128Pow5T3, ConstantLength<2>, WIDTH, RATE>::init(
            Pow5Chip::construct(config.poseidon.clone()),
            layouter.namespace(|| "init poseidon (H)"),
        )?
        .hash(
            layouter.namespace(|| "H = Poseidon(pk.x, pk.y)"),
            [pk.inner().x(), pk.inner().y()],
        )?;
        layouter.constrain_instance(h_computed.cell(), config.instance, 0)?;

        // m = Poseidon(rdv_node_hash, expires, pow_challenge), message the certificate's Schnorr signature actually signs.
        let m = Hash::<_, _, P128Pow5T3, ConstantLength<3>, WIDTH, RATE>::init(
            Pow5Chip::construct(config.poseidon.clone()),
            layouter.namespace(|| "init poseidon (m)"),
        )?
        .hash(
            layouter.namespace(|| "m = Poseidon(rdv_hash, expires, challenge)"),
            [rdv_hash_cell, expires_cell, challenge_cell],
        )?;

        // Constraint 3: Schnorr signature (R, s) verifies under pk over m.
        let r_point = NonIdentityPoint::new(ecc_chip.clone(), layouter.namespace(|| "witness R"), self.r_point)?;
        let e = Hash::<_, _, P128Pow5T3, ConstantLength<3>, WIDTH, RATE>::init(
            Pow5Chip::construct(config.poseidon.clone()),
            layouter.namespace(|| "init poseidon (e)"),
        )?
        .hash(
            layouter.namespace(|| "e = Poseidon(R.x, pk.x, m)"),
            [r_point.inner().x(), pk.inner().x(), m],
        )?;

        let s_scalar = ScalarFixed::new(ecc_chip.clone(), layouter.namespace(|| "s"), self.s)?;
        let (s_g, _) = generator.mul(layouter.namespace(|| "[s]G"), s_scalar)?;

        let e_scalar = ScalarVar::from_base(ecc_chip.clone(), layouter.namespace(|| "e as scalar"), &e)?;
        let (e_pk, _) = pk.mul(layouter.namespace(|| "[e]pk"), e_scalar)?;
        let rhs = r_point.add(layouter.namespace(|| "R + [e]pk"), &e_pk)?;

        s_g.constrain_equal(layouter.namespace(|| "verify"), &rhs)
    }
}

// Off-circuit reference computation of m = Poseidon(rdv_hash, expires, challenge),
// shared by prover (to sign) and by test/certificate-building code (to build the pulic-instance vector).
pub fn envelope_message(
    rdv_node_hash: pallas::Base,
    expires: pallas::Base,
    pow_challenge: pallas::Base,
) -> pallas::Base {
    halo2_gadgets::poseidon::primitives::Hash::<_, P128Pow5T3, ConstantLength<3>, WIDTH, RATE>::init()
        .hash([rdv_node_hash, expires, pow_challenge])
}

// Off-circuit reference computation of H = Poseidon(pk.x, pk.y).
pub fn hs_hash(pk: pallas::Affine) -> pallas::Base {
    use halo2_proofs::arithmetic::CurveAffine;
    let coords = pk.coordinates().unwrap();
    halo2_gadgets::poseidon::primitives::Hash::<_, P128Pow5T3, ConstantLength<2>, WIDTH, RATE>::init()
        .hash([*coords.x(), *coords.y()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schnorr::sign;
    use group::ff::Field;
    use halo2_proofs::dev::MockProver;
    use rand::rngs::OsRng;

    fn valid_instance(
        sk: pallas::Scalar,
        rdv_node_hash: pallas::Base,
        expires: pallas::Base,
        pow_challenge: pallas::Base,
    ) -> (CertificateCircuit, Vec<pallas::Base>) {
        let pk = derive_pk(sk);
        let h = hs_hash(pk);
        let m = envelope_message(rdv_node_hash, expires, pow_challenge);
        let k = pallas::Scalar::random(OsRng);
        let (r_point, s) = sign(sk, k, m);

        let circuit = CertificateCircuit {
            sk: Value::known(sk),
            r_point: Value::known(r_point),
            s: Value::known(s),
        };
        (circuit, vec![h, rdv_node_hash, expires, pow_challenge])
    }

    #[test]
    fn mock_valid_certificate_accepted() {
        let sk = pallas::Scalar::random(OsRng);
        let rdv_node_hash = pallas::Base::random(OsRng);
        let expires = pallas::Base::from(1_800_000_000u64);
        let pow_challenge = pallas::Base::random(OsRng);

        let (circuit, instance) = valid_instance(sk, rdv_node_hash, expires, pow_challenge);
        let prover = MockProver::run(crate::certificate::K, &circuit, vec![instance]).unwrap();
        assert_eq!(prover.verify(), Ok(()));
    }

    #[test]
    fn mock_tampered_expiry_rejected() {
        let sk = pallas::Scalar::random(OsRng);
        let rdv_node_hash = pallas::Base::random(OsRng);
        let expires = pallas::Base::from(1_800_000_000u64);
        let pow_challenge = pallas::Base::random(OsRng);

        let (circuit, mut instance) = valid_instance(sk, rdv_node_hash, expires, pow_challenge);
        // Tamper with expires (instance index 2) after the certificate was
        // signed. The envelope message binds it, so this must fail.
        instance[2] = pallas::Base::from(9_999_999_999u64);
        let prover = MockProver::run(crate::certificate::K, &circuit, vec![instance]).unwrap();
        assert!(prover.verify().is_err());
    }

    #[test]
    fn mock_hash_not_matching_key_rejected() {
        let sk = pallas::Scalar::random(OsRng);
        let rdv_node_hash = pallas::Base::random(OsRng);
        let expires = pallas::Base::from(1_800_000_000u64);
        let pow_challenge = pallas::Base::random(OsRng);

        let (circuit, mut instance) = valid_instance(sk, rdv_node_hash, expires, pow_challenge);
        // Claim a hash that doesn't correspond to sk's actual pk: an
        // attacker claiming ownership of a hidden service hash it doesn't control
        instance[0] = pallas::Base::random(OsRng);
        let prover = MockProver::run(crate::certificate::K, &circuit, vec![instance]).unwrap();
        assert!(prover.verify().is_err());
    }
}
