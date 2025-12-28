pub fn check_hash(hash: &[u8; 32], difficulty: u64) -> bool {
    let mut carry: u128 = 0;

    // walk through 32 bytes 8 at a time (u64 chunks)
    for i in (0..32).step_by(8) {
        let part = u64::from_le_bytes(hash[i..i+8].try_into().unwrap()) as u128;
        let prod = part * difficulty as u128 + carry;
        carry = prod >> 64;
    }

    // if carry == 0 after processing all chunks => hash meets difficulty
    carry == 0
}

