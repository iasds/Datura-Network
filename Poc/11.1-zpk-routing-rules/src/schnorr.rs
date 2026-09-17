// Constraint 3 of the certificate circuit: prove that a Schnorr signature
// (R, s) over a public message m verifies under pk, where pk = [sk]G
// (constraint 1, reused from dlog), w/o revealing pk itself.

// The verification equation [s]G = R + [e]pk (with e = Poseidon(R.x, pk.x, m))
// has to be checked entirely inside the circuit. [e]pk is a variable-base multiplication where the scalar e is
// a circuit-computed base-field (Poseidon output) element.
// ScalarVar::from_base (used in Orchard for [ivk] g_d_old) is the gadget that lets a base-field element
// be used as a variable-base scalar. (makes this possible without any non-native field arithmetic)

use ff::PrimeField;
use halo2_proofs::arithmetic::CurveAffine;
use pasta_curves::pallas;

use crate::dlog::{
    HsEccChip, HsEccConfig, HsGenerator, configure_ecc, derive_pk, load_plain_range_table,
};
use halo2_gadgets::ecc::{FixedPoint, NonIdentityPoint, ScalarFixed, ScalarVar};
use halo2_gadgets::poseidon::{
    Hash, Pow5Chip, Pow5Config,
    primitives::{self as poseidon, ConstantLength, P128Pow5T3},
};
use halo2_proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance, TableColumn},
};

const WIDTH: usize = 3;
const RATE: usize = 2;
const L: usize = 3; // e = Poseidon(R.x, pk.x, m)

// Off-circuit reference computation of the challenge, used to produce signatures (in tests, and by Node B)
// and, in-circuit, to verify them. Must stay identical to the in-circuit gadget sequence.
pub fn challenge(r_point: pallas::Affine, pk: pallas::Affine, m: pallas::Base) -> pallas::Base {
    let r_x = *r_point.coordinates().unwrap().x();
    let pk_x = *pk.coordinates().unwrap().x();
    poseidon::Hash::<_, P128Pow5T3, ConstantLength<L>, WIDTH, RATE>::init().hash([r_x, pk_x, m])
}

// Off-circuit Schnorr signing, for building test fixtures and for Node B.
// Returns (R, s).
pub fn sign(
    sk: pallas::Scalar,
    k: pallas::Scalar,
    m: pallas::Base,
) -> (pallas::Affine, pallas::Scalar) {
    let pk = derive_pk(sk);
    let r_point = derive_pk(k); // [k]G, reusing the same fixed generator
    let e = challenge(r_point, pk, m);
    // e is used as a scalar-field/Fq multiplier.
    // Pallas is constructed so that Fp's modulus is smaller than Fq's, so
    // this reinterpretation is safe and matches ScalarVar::from_base.
    let e_scalar = pallas::Scalar::from_repr(e.to_repr()).unwrap();
    let s = k + e_scalar * sk;
    (r_point, s)
}

// Off-circuit Schnorr verification: [s]G == R + [e]pk. Used for the RDV node's own endorsement signature on a certificate,
// where the signer's pk is public. the RDV node's identity hash is Poseidon(pk), already known network-wide: so no
// zk is needed, just verification equation.
pub fn verify(
    pk: pallas::Affine,
    r_point: pallas::Affine,
    s: pallas::Scalar,
    m: pallas::Base,
) -> bool {
    use group::{Curve, prime::PrimeCurveAffine};
    // challenge() reads affine coordinates, which the identity doesn't have;
    // an identity pk or R is invalid in any case.
    if bool::from(pk.is_identity()) || bool::from(r_point.is_identity()) {
        return false;
    }
    let e = challenge(r_point, pk, m);
    let e_scalar = pallas::Scalar::from_repr(e.to_repr()).unwrap();
    let lhs = derive_pk(s); // [s]G
    let rhs = (r_point.to_curve() + pk * e_scalar).to_affine();
    lhs == rhs
}

// SchnorrConfig/SchnorrCircuit isolate and validate constraint 3 on its own. The
// combined certificate circuit in circuit.rs is what's actually used at runtime, so these are only exercised by this module's own tests below.
#[allow(dead_code)]
#[derive(Clone)]
pub struct SchnorrConfig {
    ecc: HsEccConfig,
    poseidon: Pow5Config<pallas::Base, WIDTH, RATE>,
    poseidon_state: [Column<Advice>; WIDTH],
    lookup_table: TableColumn,
    instance: Column<Instance>,
}

#[allow(dead_code)]
pub struct SchnorrCircuit {
    pub sk: Value<pallas::Scalar>,
    pub r_point: Value<pallas::Affine>,
    pub s: Value<pallas::Scalar>,
}

impl Circuit<pallas::Base> for SchnorrCircuit {
    type Config = SchnorrConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self {
            sk: Value::unknown(),
            r_point: Value::unknown(),
            s: Value::unknown(),
        }
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> SchnorrConfig {
        let (ecc, lookup_table) = configure_ecc(meta);

        let poseidon_state: [Column<Advice>; WIDTH] = (0..WIDTH)
            .map(|_| meta.advice_column())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let partial_sbox = meta.advice_column();
        let rc_a: [Column<halo2_proofs::plonk::Fixed>; WIDTH] = (0..WIDTH)
            .map(|_| meta.fixed_column())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let rc_b: [Column<halo2_proofs::plonk::Fixed>; WIDTH] = (0..WIDTH)
            .map(|_| meta.fixed_column())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        meta.enable_constant(rc_b[0]);
        for c in &poseidon_state {
            meta.enable_equality(*c);
        }

        let poseidon =
            Pow5Chip::configure::<P128Pow5T3>(meta, poseidon_state, partial_sbox, rc_a, rc_b);

        let instance = meta.instance_column();
        meta.enable_equality(instance);

        SchnorrConfig {
            ecc,
            poseidon,
            poseidon_state,
            lookup_table,
            instance,
        }
    }

    fn synthesize(
        &self,
        config: SchnorrConfig,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        load_plain_range_table(config.lookup_table, &mut layouter)?;

        let ecc_chip = HsEccChip::construct(
            config.ecc,
            halo2_gadgets::ecc::chip::CircuitVersion::AnchoredBase,
        );

        // pk = [sk]G: witness pk directly, and separately derive it from sk, constraining the two to match
        // This gives us pk as a NonIdentityPoint, usable as the base of the later variable-base
        // [e]pk multiplication (fixed-base mul only ever returns the possibly-identity Point type).
        let pk_value = self.sk.map(derive_pk);
        let pk = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness pk"),
            pk_value,
        )?;
        let sk_scalar = ScalarFixed::new(ecc_chip.clone(), layouter.namespace(|| "sk"), self.sk)?;
        let generator = FixedPoint::from_inner(ecc_chip.clone(), HsGenerator);
        let (pk_derived, _) = generator.mul(layouter.namespace(|| "[sk]G"), sk_scalar)?;
        pk.constrain_equal(layouter.namespace(|| "pk == [sk]G"), &pk_derived)?;

        // R, witnessed directly (computed off-circuit as [k]G for a nonce k that never appears in this circuit at all).
        let r_point = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness R"),
            self.r_point,
        )?;

        // m, brought in from the public instance.
        let m_cell = layouter.assign_region(
            || "load m",
            |mut region| {
                region.assign_advice_from_instance(
                    || "m",
                    config.instance,
                    0,
                    config.poseidon_state[0],
                    0,
                )
            },
        )?;

        // e = Poseidon(R.x, pk.x, m)
        let poseidon_chip = Pow5Chip::construct(config.poseidon.clone());
        let hasher = Hash::<_, _, P128Pow5T3, ConstantLength<L>, WIDTH, RATE>::init(
            poseidon_chip,
            layouter.namespace(|| "init poseidon"),
        )?;
        let e_cell = hasher.hash(
            layouter.namespace(|| "e = Poseidon(R.x, pk.x, m)"),
            [r_point.inner().x(), pk.inner().x(), m_cell],
        )?;

        // [s]G
        let s_scalar = ScalarFixed::new(ecc_chip.clone(), layouter.namespace(|| "s"), self.s)?;
        let (s_g, _) = generator.mul(layouter.namespace(|| "[s]G"), s_scalar)?;

        // R + [e]pk
        let e_scalar = ScalarVar::from_base(
            ecc_chip.clone(),
            layouter.namespace(|| "e as scalar"),
            &e_cell,
        )?;
        let (e_pk, _) = pk.mul(layouter.namespace(|| "[e]pk"), e_scalar)?;
        let rhs = r_point.add(layouter.namespace(|| "R + [e]pk"), &e_pk)?;

        // [s]G == R + [e]pk
        s_g.constrain_equal(layouter.namespace(|| "verify"), &rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use group::ff::Field;
    use halo2_proofs::dev::MockProver;
    use rand::rngs::OsRng;

    #[test]
    fn mock_valid_signature_accepted() {
        let sk = pallas::Scalar::random(OsRng);
        let k = pallas::Scalar::random(OsRng);
        let m = pallas::Base::random(OsRng);
        let (r_point, s) = sign(sk, k, m);

        let circuit = SchnorrCircuit {
            sk: Value::known(sk),
            r_point: Value::known(r_point),
            s: Value::known(s),
        };
        let prover = MockProver::run(crate::certificate::K, &circuit, vec![vec![m]]).unwrap();
        assert_eq!(prover.verify(), Ok(()));
    }

    #[test]
    fn mock_wrong_message_rejected() {
        let sk = pallas::Scalar::random(OsRng);
        let k = pallas::Scalar::random(OsRng);
        let m = pallas::Base::random(OsRng);
        let wrong_m = pallas::Base::random(OsRng);
        let (r_point, s) = sign(sk, k, m);

        // Circuit is given a signature for m, but the public instance claims it's for wrong_m. Must be rejected.
        let circuit = SchnorrCircuit {
            sk: Value::known(sk),
            r_point: Value::known(r_point),
            s: Value::known(s),
        };
        let prover = MockProver::run(crate::certificate::K, &circuit, vec![vec![wrong_m]]).unwrap();
        assert!(prover.verify().is_err());
    }

    #[test]
    fn mock_signature_from_wrong_key_rejected() {
        let sk = pallas::Scalar::random(OsRng);
        let other_sk = pallas::Scalar::random(OsRng);
        let k = pallas::Scalar::random(OsRng);
        let m = pallas::Base::random(OsRng);
        // Sign with a different key than the one whose pk the circuit derives.
        let (r_point, s) = sign(other_sk, k, m);

        let circuit = SchnorrCircuit {
            sk: Value::known(sk),
            r_point: Value::known(r_point),
            s: Value::known(s),
        };
        let prover = MockProver::run(crate::certificate::K, &circuit, vec![vec![m]]).unwrap();
        assert!(prover.verify().is_err());
    }
}
