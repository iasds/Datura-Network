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
    buf[0] = difficulty&0b111111;

    buf
}

fn validate_solution(vm: &RandomXVM, challenge: [u8; 16], solution: u64) -> bool {
    let mut cat = [0u8; 24];
    cat[..16].copy_from_slice(&challenge);
    cat[16..].copy_from_slice(&solution.to_le_bytes());

    let hash = vm.calculate_hash(&cat).unwrap();
    let hash_u128 = u128::from_le_bytes(hash[0..16].try_into().unwrap()) ^ u128::from_le_bytes(hash[16..32].try_into().unwrap());

    let difficulty = challenge[0]&0b111111;

    hash_u128.leading_zeros() > difficulty.into()
}

fn solve_challenge(vm: &RandomXVM, challenge: [u8; 16]) -> u64 {
    let mut solution = getrandom::u64().unwrap();
    loop {
        if validate_solution(vm, challenge, solution) { return solution };

        solution += 1;
    }
}

fn main() {
    let cache = RandomXCache::new(RandomXFlag::FLAG_JIT | RandomXFlag::FLAG_ARGON2_AVX2, &[0]).unwrap();

    println!("initializing dataset (only needs to be done once, at node startup)...");
    let now = std::time::Instant::now();
    let dataset = RandomXDataset::new(RandomXFlag::FLAG_DEFAULT, cache, 0).unwrap();
    println!("initialized, took {:.2?}\n", now.elapsed());

    let vm = RandomXVM::new(RandomXFlag::FLAG_HARD_AES | RandomXFlag::FLAG_FULL_MEM | RandomXFlag::FLAG_JIT, None, Some(dataset)).unwrap();

    let key = b"test key 000";
    let flags = RandomXFlag::get_recommended_flags();
    let cache = RandomXCache::new(flags, key).unwrap();
    let light_vm = RandomXVM::new(flags, Some(cache), None).unwrap();

    let args = env::args();
    let mut test_vm = &vm;
    if args.len() < 2 {
        println!("by default, running randomX in fast mode");
    }
    else {
        println!("running in light mode");
        test_vm = &light_vm;
    }
        
     
    for d in 0..15 {
        let challenge = create_challenge(d);
        println!("created challenge of difficulty {}", d);

        let now = std::time::Instant::now();
        println!("solving challenge {:02X?}", challenge);

        let solution = solve_challenge(test_vm, challenge);
        println!("took {:.2?} to find solution ({:X?})", now.elapsed(), solution);
        
        let now = std::time::Instant::now();
        assert!(validate_solution(test_vm, challenge, solution));
        println!("took {:.2?} to validate solution\n", now.elapsed());
    }

    let challenge = create_challenge(40);
    println!("created challenge of difficulty 40");

    let solution = getrandom::u64().unwrap();
    println!("created random solution {}", solution);
    
    let result = validate_solution(test_vm, challenge, solution);

    assert!(result == false);
    println!("solution correctly validated as {} (false)", result);
}
