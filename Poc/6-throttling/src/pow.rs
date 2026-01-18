//! copied from Pow-4.
use randomx_rs::RandomXVM;

const CHALLENGE_DIFFICULTY: u8 = 15;

pub fn create_challenge() -> [u8; 16] {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).unwrap();
    buf[0] = CHALLENGE_DIFFICULTY & 0b111111;

    buf
}

pub fn validate_solution(vm: &RandomXVM, challenge: [u8; 16], solution: [u8; 8]) -> bool {
    let mut cat = [0u8; 24];
    cat[..16].copy_from_slice(&challenge);
    cat[16..].copy_from_slice(&solution);

    let hash = vm.calculate_hash(&cat).unwrap();
    let hash_u128 = u128::from_le_bytes(hash[0..16].try_into().unwrap())
        ^ u128::from_le_bytes(hash[16..32].try_into().unwrap());

    let difficulty = challenge[0] & 0b111111;

    hash_u128.leading_zeros() > difficulty.into()
}
