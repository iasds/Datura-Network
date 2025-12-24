use equix::*;
use blake2b_simd::Params;
use std::thread;
use std::process::exit;
use std::io::{stdin,stdout,Write};
use std::time::Duration;
use std::sync::atomic::{AtomicBool,Ordering};
use std::sync::Arc;

const BENCHMARK_SAMPLES: u32 = 32;

fn create_challenge(effort: u32) -> u128 {
	// pack random number and effort into challenge
	(getrandom::u64().unwrap() as u128) << 64 |
	(getrandom::u32().unwrap() as u128) << 32 |
	effort as u128
}

fn solve_challenge(num_threads: usize, challenge: u128) -> [u8; 24] {
	let effort: u32 = (challenge&0xffffffff) as u32;

	let search_pos: u64 = getrandom::u64().unwrap();
	let search_inc = u64::MAX / num_threads as u64;
	
	let mut handles: Vec<thread::JoinHandle<Option<[u8; 24]>>> = Vec::with_capacity(num_threads);
	let done = Arc::new(AtomicBool::new(false));

	for t in 0..num_threads {
		// each thread needs to search as far away from each other as they can
		let mut salt: u64 = search_pos + search_inc*(t as u64);

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
					Ok(v) => v,
					Err(_) => continue,
				};

				let equix_solutions = equix.solve_with_memory(&mut mem);
				if equix_solutions.is_empty() { continue; }

				seed[24..40].copy_from_slice(&equix_solutions[0].to_bytes());

				let hash_u32 = u32::from_le_bytes(hasher.hash(&seed).as_bytes().try_into().unwrap());

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
	for h in handles {
		let r = h.join().unwrap();
		if r.is_some() {
			return r.unwrap();
		}
	}

	assert!(false);
	return [0u8; 24];
}

fn verify_solution(effort: u32, challenge: u128, solution: [u8; 24]) -> bool {
	// extract effort parameter from challenge
	if ((challenge&0xffffffff) as u32) != effort { return false; };

	let mut seed = [0u8; 40];

	// pack challenge and solution for hashing
	seed[..16].copy_from_slice(&challenge.to_le_bytes());
	seed[16..].copy_from_slice(&solution);

	let hash_u32: u32 = u32::from_le_bytes(
		Params::new().hash_length(4).hash(&seed)
		.as_bytes().try_into().unwrap()
	);

	// fail if requirements are not met
	if (hash_u32 as u64) * (effort as u64) >= u32::MAX.into() { return false; }

	// fail or succeed depending on if Equi-X solution is verified
	return verify_bytes(&seed[..24], &seed[24..].try_into().unwrap()).is_ok();
}

fn main() {
	let num_threads = thread::available_parallelism().unwrap().get();
	print!("How many threads should i use? (Press Enter for default of {}): ", num_threads);	
	let _ = stdout().flush().unwrap();

	// get thread selection string
	let mut selection_string = String::new();
	stdin().read_line(&mut selection_string).unwrap();
	let picked_num_threads: usize = match selection_string.trim().parse() {
		Ok(num) => num,
		Err(_) => num_threads.into(),
	};

	// parse validity of thread selection
	if picked_num_threads < 1 || picked_num_threads > num_threads { println!("Thread amount {} is out of bounds", picked_num_threads); exit(1); }
	println!("\nRunning with {} thread(s)\n", picked_num_threads);

	// benchmark at various effort values
	for i in 0..8 {
		let d = i * 64;

		let mut solve_sum: Duration = Duration::from_secs(0);
		let mut verify_sum: Duration = Duration::from_secs(0);

		print!("Solving {} challenges of effort {} ", BENCHMARK_SAMPLES, d); let _ = stdout().flush().unwrap();
		for _ in 0..BENCHMARK_SAMPLES {
			let challenge = create_challenge(d);

			let now = std::time::Instant::now();
			let solution = solve_challenge(picked_num_threads, challenge);
			let took = now.elapsed();
			solve_sum += took;
	
			let now = std::time::Instant::now();
			assert!(verify_solution(d, challenge, solution));
			let took = now.elapsed();
			verify_sum += took;

			print!("."); let _ = stdout().flush().unwrap();
		}
		println!("\nSolution time avg: {:.2?}\nVerification time avg: {:.2?}\n", solve_sum / BENCHMARK_SAMPLES, verify_sum / BENCHMARK_SAMPLES);
	}


	// test that a random solution fails verification
	{
		let test_effort = 10000;

		let challenge = create_challenge(test_effort);
		println!("Created challenge of effort 10000");

		let mut random_solution = [0u8; 24];
		getrandom::fill(&mut random_solution).unwrap();
		println!("Created random solution {:02X?}", random_solution);
	
		let result = verify_solution(test_effort, challenge, random_solution);

		assert!(result == false);
		println!("Solution correctly verified as {} (false)", result);
	}
}
