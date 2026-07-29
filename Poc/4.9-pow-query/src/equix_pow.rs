use blake2b_simd::Params;
use equix::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

pub fn create_challenge(effort: u32) -> u128 {
    // pack random number and effort into challenge
    (getrandom::u64().unwrap() as u128) << 64
        | (getrandom::u32().unwrap() as u128) << 32
        | effort as u128
}

pub fn get_challenge_effort(challenge: u128) -> u32 {
    (challenge & 0xffffffff) as u32
}

pub fn solve_challenge(num_threads: usize, challenge: u128) -> [u8; 24] {
    let effort = get_challenge_effort(challenge);

    let search_pos: u64 = getrandom::u64().unwrap();
    let search_inc = u64::MAX / num_threads as u64;

    let mut handles: Vec<thread::JoinHandle<Option<[u8; 24]>>> = Vec::with_capacity(num_threads);
    let done = Arc::new(AtomicBool::new(false));

    for t in 0..num_threads {
        // each thread needs to search as far away from each other as they can
        let mut salt: u64 = search_pos + search_inc * (t as u64);

        let done_local = done.clone();
        handles.push(thread::spawn(move || {
            let mut hasher = Params::new();
            hasher.hash_length(4);

            let mut mem = SolverMemory::new();

            // repeat Equi-X solutions until one matches requirements
            loop {
                salt += 1;

                let mut seed = [0u8; 40];

                // pack challenge and salt to attempt Equi-X solution for
                seed[..16].copy_from_slice(&challenge.to_le_bytes());
                seed[16..24].copy_from_slice(&salt.to_le_bytes());

                let equix = match EquiX::new(&seed[..24]) {
                    Ok(equix) => equix,
                    Err(_) => continue,
                };

                let equix_solutions = equix.solve_with_memory(&mut mem);
                if equix_solutions.is_empty() {
                    continue;
                }

                seed[24..40].copy_from_slice(&equix_solutions[0].to_bytes());

                let hash_u32 =
                    u32::from_le_bytes(hasher.hash(&seed).as_bytes().try_into().unwrap());

                // if requirements are met, signal other threads to stop, then return
                if (hash_u32 as u64) * (effort as u64) < u32::MAX.into() {
                    done_local.store(true, Ordering::Release);
                    return Some(seed[16..40].try_into().unwrap());
                }

                if done_local.load(Ordering::Acquire) {
                    return None;
                }
            }
        }));
    }

    // join each handle until one returns a valid value, then return that value
    for handle in handles {
        let join_result = handle.join().unwrap();
        if let Some(solution) = join_result {
            return solution;
        }
    }

    unreachable!(
        "all solver threads returned None, but a thread only returns None after observing done == true, which is only ever set by a thread that already returned Some(solution)"
    )
}

pub fn verify_solution(effort: u32, challenge: u128, solution: [u8; 24]) -> bool {
    // extract effort parameter from challenge
    if get_challenge_effort(challenge) != effort {
        return false;
    };

    let mut seed = [0u8; 40];

    // pack challenge and solution for hashing
    seed[..16].copy_from_slice(&challenge.to_le_bytes());
    seed[16..].copy_from_slice(&solution);

    let hash_u32: u32 = u32::from_le_bytes(
        Params::new()
            .hash_length(4)
            .hash(&seed)
            .as_bytes()
            .try_into()
            .unwrap(),
    );

    // fail if requirements are not met
    if (hash_u32 as u64) * (effort as u64) >= u32::MAX.into() {
        return false;
    }

    // fail or succeed depending on if Equi-X solution is verified
    verify_bytes(&seed[..24], &seed[24..].try_into().unwrap()).is_ok()
}
