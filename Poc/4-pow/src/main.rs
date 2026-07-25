use randomx_rs::*;
use std::env;

/*
Challenge Format: 128-bit
     1 byte  difficulty (number of starting zero bits) (only 6-bits are used)
    15 bytes random bits

Solution Format: 64-bit
    8 bytes salt
*/

fn create_challenge(difficulty: u8) -> [u8; 16] {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).unwrap();
    buf[0] = difficulty & 0b111111;

    buf
}

fn validate_solution(vm: &RandomXVM, challenge: [u8; 16], solution: u64) -> bool {
    let mut seed = [0u8; 24];
    seed[..16].copy_from_slice(&challenge);
    seed[16..].copy_from_slice(&solution.to_le_bytes());

    let hash = vm.calculate_hash(&seed).unwrap();
    let hash_u128 = u128::from_le_bytes(hash[0..16].try_into().unwrap())
        ^ u128::from_le_bytes(hash[16..32].try_into().unwrap());

    let difficulty = challenge[0] & 0b111111;

    hash_u128.leading_zeros() > difficulty.into()
}

fn solve_challenge(vm: &RandomXVM, challenge: [u8; 16]) -> u64 {
    let mut solution = getrandom::u64().unwrap();
    loop {
        if validate_solution(vm, challenge, solution) {
            return solution;
        };

        solution += 1;
    }
}

fn main() {
    /*
     * got challenge JobData { blob: "1010ebc2beca060def36c3eb9b1f32cb8e26e8880cdee27124c52800e8e984087579aaba8b10d8000000d2ad79456f7b9a22de7b63085297d19502d8d406720b3761047071e82878fb9d0933", job_id: "7", target: "37860000", algo: "rx/0", height: 3574527, seed_hash: "491c63749eea49f1c01ce7dc29934437ea725b41fcd13c5456433156225f17c8" }
     */
    let _test_blob = b"1010ebc2beca060def36c3eb9b1f32cb8e26e8880cdee27124c52800e8e984087579aaba8b10d8000000d2ad79456f7b9a22de7b63085297d19502d8d406720b3761047071e82878fb9d0933";
    let seed_hash = b"491c63749eea49f1c01ce7dc29934437ea725b41fcd13c5456433156225f17c8";

    let cache = RandomXCache::new(RandomXFlag::FLAG_DEFAULT, seed_hash).unwrap();

    let now = std::time::Instant::now();

    let vm = if env::args().len() > 1 {
        RandomXVM::new(RandomXFlag::FLAG_DEFAULT, Some(cache), None).unwrap()
    } else {
        println!("initializing dataset (only needs to be done once, at node startup)...");
        let dataset = RandomXDataset::new(RandomXFlag::FLAG_DEFAULT, cache, 0).unwrap();
        println!("initialized, took {:.2?}\n", now.elapsed());

        RandomXVM::new(
            RandomXFlag::FLAG_HARD_AES | RandomXFlag::FLAG_FULL_MEM | RandomXFlag::FLAG_JIT,
            None,
            Some(dataset),
        )
        .unwrap()
    };

    for difficulty in 0..15 {
        let challenge = create_challenge(difficulty);
        println!("created challenge of difficulty {}", difficulty);

        let now = std::time::Instant::now();
        println!("solving challenge {:02X?}", challenge);

        let solution = solve_challenge(&vm, challenge);
        println!(
            "took {:.2?} to find solution ({:X?})",
            now.elapsed(),
            solution
        );

        let now = std::time::Instant::now();
        assert!(validate_solution(&vm, challenge, solution));
        println!("took {:.2?} to validate solution\n", now.elapsed());
    }

    let challenge = create_challenge(40);
    println!("created challenge of difficulty 40");

    let solution = getrandom::u64().unwrap();
    println!("created random solution {}", solution);

    let result = validate_solution(&vm, challenge, solution);

    assert!(!result);
    println!("solution correctly validated as {} (false)", result);
}
