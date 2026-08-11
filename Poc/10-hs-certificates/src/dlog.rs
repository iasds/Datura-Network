// Constraint 1 of the certificate circuit: prove knowledge of sk such that
// pk = [sk]G, for a fixed public generator G (the pasta pallas curve
// generator), exposing pk as a public instance.

// NOTE:
// This mirrors halo2_gadgets::ecc's own internal TestFixedBases test
// circuit, and the fixed-base mul pattern Orchard uses for
// [alpha] SpendAuthG (orchard/src/circuit.rs).

use std::sync::LazyLock;

use ff::PrimeField;
use group::{Curve, Group};
use pasta_curves::pallas;

use halo2_gadgets::ecc::{
    FixedPoint, FixedPoints, ScalarFixed,
    chip::{
        BaseFieldElem, EccChip, EccConfig, FixedPoint as FixedPointConstants, FullScalar, H,
        NUM_WINDOWS, ShortScalar, find_zs_and_us,
    },
};
use halo2_gadgets::utilities::lookup_range_check::{
    LookupRangeCheck, PallasLookupRangeCheckConfig,
};
use halo2_proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Fixed, Instance, TableColumn},
};

// Number of bits in the ECC chip's plain range-check lookup table.

// LookupRangeCheckConfig::load_range_check_table (the method that would
// normally fill this table) is #[cfg(test)]-gated inside halo2_gadgets and
// not part of its public API. Production code (Orchard) instead fills this
// table as a side effect of loading a Sinsemilla generator table, which this
// circuit has no other use for. Rather than configure an unused Sinsemilla
// chip solely for that side effect, load_plain_range_table below reproduces
// the upstream logic directly against our own TableColumn handle.

// ngl, this was annoying to write
const RANGE_CHECK_K: usize = halo2_gadgets::sinsemilla::primitives::K;

pub(crate) fn load_plain_range_table(
    table_idx: TableColumn,
    layouter: &mut impl Layouter<pallas::Base>,
) -> Result<(), Error> {
    layouter.assign_table(
        || "plain range-check table_idx",
        |mut table| {
            for index in 0..(1 << RANGE_CHECK_K) {
                table.assign_cell(
                    || "table_idx",
                    table_idx,
                    index,
                    || Value::known(pallas::Base::from(index as u64)),
                )?;
            }
            Ok(())
        },
    )
}

static GENERATOR: LazyLock<pallas::Affine> =
    LazyLock::new(|| pallas::Point::generator().to_affine());
static ZS_AND_US: LazyLock<Vec<(u64, [pallas::Base; H])>> =
    LazyLock::new(|| find_zs_and_us(*GENERATOR, NUM_WINDOWS).unwrap());

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct HsGenerator;

// Short and BaseField exist only because the FixedPoints trait demands all
// three scalar kinds. No circuit in this crate does a short-scalar or
// base-field fixed-base multiplication, so their constants are stubs; using
// either gadget would need real precomputed windows here first.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Short;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct BaseField;

impl FixedPointConstants<pallas::Affine> for HsGenerator {
    type FixedScalarKind = FullScalar;
    fn generator(&self) -> pallas::Affine {
        *GENERATOR
    }
    fn u(&self) -> Vec<[[u8; 32]; H]> {
        ZS_AND_US
            .iter()
            .map(|(_, us)| {
                let mut out = [[0u8; 32]; H];
                for (dst, u) in out.iter_mut().zip(us.iter()) {
                    *dst = u.to_repr();
                }
                out
            })
            .collect()
    }
    fn z(&self) -> Vec<u64> {
        ZS_AND_US.iter().map(|(z, _)| *z).collect()
    }
}

impl FixedPointConstants<pallas::Affine> for Short {
    type FixedScalarKind = ShortScalar;
    fn generator(&self) -> pallas::Affine {
        unimplemented!("short-scalar fixed-base mul is not used in this crate")
    }
    fn u(&self) -> Vec<[[u8; 32]; H]> {
        unimplemented!("short-scalar fixed-base mul is not used in this crate")
    }
    fn z(&self) -> Vec<u64> {
        unimplemented!("short-scalar fixed-base mul is not used in this crate")
    }
}

impl FixedPointConstants<pallas::Affine> for BaseField {
    type FixedScalarKind = BaseFieldElem;
    fn generator(&self) -> pallas::Affine {
        unimplemented!("base-field fixed-base mul is not used in this crate")
    }
    fn u(&self) -> Vec<[[u8; 32]; H]> {
        unimplemented!("base-field fixed-base mul is not used in this crate")
    }
    fn z(&self) -> Vec<u64> {
        unimplemented!("base-field fixed-base mul is not used in this crate")
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct HsFixedBases;

impl FixedPoints<pallas::Affine> for HsFixedBases {
    type FullScalar = HsGenerator;
    type ShortScalar = Short;
    type Base = BaseField;
}

pub type HsEccChip = EccChip<HsFixedBases, PallasLookupRangeCheckConfig>;
pub type HsEccConfig = EccConfig<HsFixedBases, PallasLookupRangeCheckConfig>;

// DlogConfig/DlogCircuit exist to isolate and validate constraint 1 on its
// own. The combined certificate circuit in circuit.rs is
// what's actually used at runtime, so these are only exercised by this
// module's own tests below.
#[allow(dead_code)]
#[derive(Clone)]
pub struct DlogConfig {
    ecc: HsEccConfig,
    lookup_table: TableColumn,
    instance: Column<Instance>,
}

// Standard 10-advice / lookup / lagrange-coeff ecc chip setup, shared by
// every circuit in this crate that needs elliptic-curve gates. Returns the
// raw lookup_table column too, b/c loading it is our own responsibility.
pub fn configure_ecc(meta: &mut ConstraintSystem<pallas::Base>) -> (HsEccConfig, TableColumn) {
    let advices: [Column<Advice>; 10] = (0..10)
        .map(|_| meta.advice_column())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let lookup_table = meta.lookup_table_column();
    let lagrange_coeffs: [Column<Fixed>; 8] = (0..8)
        .map(|_| meta.fixed_column())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let constants = meta.fixed_column();
    meta.enable_constant(constants);

    let range_check = PallasLookupRangeCheckConfig::configure(meta, advices[9], lookup_table);
    let ecc_config = HsEccChip::configure(meta, advices, lagrange_coeffs, range_check);
    (ecc_config, lookup_table)
}

#[allow(dead_code)]
pub struct DlogCircuit {
    pub sk: Value<pallas::Scalar>,
}

impl Circuit<pallas::Base> for DlogCircuit {
    type Config = DlogConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self {
            sk: Value::unknown(),
        }
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> DlogConfig {
        let (ecc, lookup_table) = configure_ecc(meta);
        let instance = meta.instance_column();
        meta.enable_equality(instance);
        DlogConfig {
            ecc,
            lookup_table,
            instance,
        }
    }

    fn synthesize(
        &self,
        config: DlogConfig,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        load_plain_range_table(config.lookup_table, &mut layouter)?;

        let chip = HsEccChip::construct(
            config.ecc,
            halo2_gadgets::ecc::chip::CircuitVersion::AnchoredBase,
        );

        let sk = ScalarFixed::new(chip.clone(), layouter.namespace(|| "sk"), self.sk)?;
        let generator = FixedPoint::from_inner(chip.clone(), HsGenerator);
        let (pk, _) = generator.mul(layouter.namespace(|| "pk = [sk]G"), sk)?;

        layouter.constrain_instance(pk.inner().x().cell(), config.instance, 0)?;
        layouter.constrain_instance(pk.inner().y().cell(), config.instance, 1)
    }
}

// Non-circuit reference computation of pk = [sk]g, for building the
// public-instance vector the prover/verifier need.
pub fn derive_pk(sk: pallas::Scalar) -> pallas::Affine {
    (*GENERATOR * sk).to_affine()
}

#[cfg(test)]
mod tests {
    use super::*;
    use group::ff::Field;
    use halo2_proofs::{arithmetic::CurveAffine, dev::MockProver};
    use rand::rngs::OsRng;

    #[test]
    fn mock_prove_and_verify() {
        let sk = pallas::Scalar::random(OsRng);
        let pk = derive_pk(sk).coordinates().unwrap();

        let circuit = DlogCircuit {
            sk: Value::known(sk),
        };
        let prover = MockProver::run(
            crate::certificate::K,
            &circuit,
            vec![vec![*pk.x(), *pk.y()]],
        )
        .unwrap();
        assert_eq!(prover.verify(), Ok(()));
    }

    #[test]
    fn mock_prove_wrong_pk_rejected() {
        let sk = pallas::Scalar::random(OsRng);
        let wrong_pk = derive_pk(pallas::Scalar::random(OsRng))
            .coordinates()
            .unwrap();

        let circuit = DlogCircuit {
            sk: Value::known(sk),
        };
        let prover = MockProver::run(
            crate::certificate::K,
            &circuit,
            vec![vec![*wrong_pk.x(), *wrong_pk.y()]],
        )
        .unwrap();
        assert!(prover.verify().is_err());
    }
}
