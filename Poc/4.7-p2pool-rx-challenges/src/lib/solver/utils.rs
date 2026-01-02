use randomx_rs::*;
use crate::SolverMode;

///Calculate hash difficulty from a hash for filtering and routing shares
pub fn hash_to_difficulty(hash: &[u8; 32]) -> u64 {
    let mut parts = [0u64; 4];
    for i in 0..4 {
        parts[i] = u64::from_be_bytes(hash[i * 8..i * 8 + 8].try_into().unwrap());
    }

    // special case: all zero hash => infinite difficulty, clamp to u64::MAX
    if parts.iter().all(|&x| x == 0) {
        return u64::MAX;
    }

    // We compute (2^256 - 1) // hash but only need the top 64 bits of the quotient.
    //
    // Since difficulty outputs are typically capped to u64, we can use a small
    // big-integer divide in place. This avoids requiring a full bigint library.

    // Numerator = (2^256-1) represented as u64[4] = all ones
    let mut num = [u64::MAX; 4];
    let den = parts;

    // manual big integer division: only compute the top 64-bit word of the quotient
    // (classic restoring division adapted to fixed 4-word width)
    let mut quotient_hi: u64 = 0;
    for _ in 0..64 {
        // left-shift numerator by 1
        let mut carry = 0u64;
        for x in num.iter_mut().rev() {
            let next = (*x >> 63) & 1;
            *x = (*x << 1) | carry;
            carry = next;
        }

        quotient_hi <<= 1;

        // compare num and den
        let ge = num >= den;
        if ge {
            // num -= den
            let mut borrow = 0u64;
            for (x, y) in num.iter_mut().rev().zip(den.iter().rev()) {
                let (nx, b) = x.overflowing_sub(*y + borrow);
                *x = nx;
                borrow = b as u64;
            }
            quotient_hi |= 1;
        }
    }

    quotient_hi
}

pub fn get_flags(mode: SolverMode) -> RandomXFlag {
    RandomXFlag::get_recommended_flags() | {
        if mode == SolverMode::Fast {
            RandomXFlag::FLAG_FULL_MEM
        }
        else {
            RandomXFlag::empty()
        }
    }
}

pub fn get_difficulty(hex_str: &str) -> Result<u64, String> {
    
    let compact = u32::from_str_radix(hex_str, 16)
        .map_err(|e| format!("Invalid hex string: {}", e))?;
    
    // Extract size (number of bytes) from the most significant byte
    let size = (compact >> 24) as usize;
    
    // Extract the mantissa (lower 3 bytes)
    let mantissa = compact & 0x00FFFFFF;
    
    // Calculate the full target
    let target = if size <= 3 {
        // If size is 3 or less, shift right
        (mantissa as u64) >> (8 * (3 - size))
    } else {
        // Otherwise shift left
        (mantissa as u64) << (8 * (size - 3))
    };
    
    // Difficulty is max_target / target
    // For Monero, max_target is 2^64 - 1
    if target == 0 {
        return Err("Target cannot be zero".to_string());
    }
    
    let difficulty = u64::MAX / target;
    
    Ok(difficulty)
}
