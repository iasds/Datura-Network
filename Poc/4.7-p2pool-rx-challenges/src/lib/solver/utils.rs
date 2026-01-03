use crate::solver::SolverMode;
use num_bigint::BigUint;
use randomx_rs::*;
use num_traits::identities::{One,Zero};


///Calculate hash difficulty from a hash for filtering and routing shares
pub fn hash_to_difficulty(hash_le: &[u8; 32]) -> u64 {
        let hash = BigUint::from_bytes_le(hash_le);

    // max_target = 2^256 − 1
    let max_target = (BigUint::one() << 256) - BigUint::one();

    let difficulty: BigUint = max_target / hash;

    difficulty
        .try_into()
        .map_err(|_| "difficulty does not fit in u64".to_string()).unwrap()
}

pub fn get_flags(mode: SolverMode) -> RandomXFlag {
    RandomXFlag::get_recommended_flags() | {
        if mode == SolverMode::Fast {
            RandomXFlag::FLAG_FULL_MEM
        } else {
            RandomXFlag::empty()
        }
    }
}

pub fn get_difficulty(hex_le: &str) -> Result<u64, String> {

        // 1) Parse little-endian compact
    let compact_le =
        u32::from_str_radix(hex_le, 16).map_err(|e| format!("invalid hex: {e}"))?;
    let compact = u32::from_le(compact_le);

    let size = (compact >> 24) as usize;
    let mantissa = compact & 0x00ff_ffff;

    if mantissa == 0 {
        return Err("invalid compact target (mantissa = 0)".into());
    }

    // 2) Expand compact → 256-bit target
    let target = if size <= 3 {
        BigUint::from(mantissa) >> (8 * (3 - size))
    } else {
        BigUint::from(mantissa) << (8 * (size - 3))
    };

    if target.is_zero() {
        return Err("expanded target is zero".into());
    }

    // 3) Compute difficulty = (2^256 − 1) / target
    let max_target = (BigUint::one() << 256) - BigUint::one();
    let difficulty: BigUint = &max_target / target;

    Ok(difficulty
        .try_into()
        .map_err(|_| "difficulty does not fit in u64".to_string())?)
}

pub fn difficulty_to_target(difficulty: u64) -> String {
    let max_target = (BigUint::one() << 256) - BigUint::one();
    let target: BigUint = &max_target / difficulty;
    hex::encode(&target.to_bytes_le())
}

mod tests {
    use super::*;
    #[test]
    pub fn test_diff_to_target() {
        println!("diff to target for min: {}",difficulty_to_target(1));
    }
}
