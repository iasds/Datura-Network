//! copied from Pow-4.
use rand::Rng;
use randomx_rs::{RandomXCache, RandomXDataset, RandomXError, RandomXFlag, RandomXVM};

const CHALLENGE_DIFFICULTY: u8 = 15;
pub const SEED_HASH: &[u8; 64] =
    b"1a803c1f384ff8b3cb35597b8d3364d32978e4aaa7f96ca894917b6d1d473fda";

pub fn create_challenge() -> [u8; 16] {
    let mut buf = [0u8; 16];
    rand::rng().fill(&mut buf);
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

pub fn create_vm() -> Result<RandomXVM, RandomXError> {
    let cache = RandomXCache::new(RandomXFlag::FLAG_DEFAULT, SEED_HASH)?;
    let dataset = RandomXDataset::new(RandomXFlag::FLAG_DEFAULT, cache, 0)?;
    RandomXVM::new(
        RandomXFlag::FLAG_HARD_AES | RandomXFlag::FLAG_FULL_MEM | RandomXFlag::FLAG_JIT,
        None,
        Some(dataset),
    )
}
