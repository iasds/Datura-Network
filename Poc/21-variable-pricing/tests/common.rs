use rand::Rng;

/// Represents a PoW solution attempt from a client
// `client_id` and `timestamp` are read by `pow_integration_test.rs` but not by
// `attacker_simulation_test.rs`; since each integration test binary compiles this
// shared module separately, clippy's per-binary dead-code analysis flags them here.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PowSolution {
    pub client_id: u64,
    pub nonce: u64,
    pub timestamp: u64,
}

/// Mock PoW verifier that validates solutions
pub struct PowVerifier {
    difficulty: u32, // difficulty in leading zero bits
}

impl PowVerifier {
    pub fn new(difficulty: u32) -> Self {
        PowVerifier { difficulty }
    }

    /// Verify a PoW solution using simplified difficulty check
    /// For testing: nonce % 256 must be less than 256/difficulty
    /// Higher difficulty = stricter requirement
    pub fn verify(&self, solution: &PowSolution) -> bool {
        // Simplified: treat nonce modulo as difficulty level
        // difficulty 1 = accept if (nonce % 256) < 128
        // difficulty 2 = accept if (nonce % 256) < 64
        // difficulty 3 = accept if (nonce % 256) < 32
        let threshold = 256u32 >> self.difficulty;
        let work_level = (solution.nonce % 256) as u32;
        work_level < threshold
    }
}

/// Client performing PoW work
#[derive(Debug, Clone)]
pub struct PowClient {
    pub id: u64,
    pub solutions: Vec<PowSolution>,
    pub total_work: u64,
}

impl PowClient {
    pub fn new(id: u64) -> Self {
        PowClient {
            id,
            solutions: Vec::new(),
            total_work: 0,
        }
    }

    /// Client attempts to generate PoW solutions
    /// Uses rand to generate pseudo-solutions and mock hash verification
    pub fn generate_solutions(
        &mut self,
        attempt_count: u64,
        difficulty: u32,
        mut _rng: impl Rng,
    ) -> Vec<PowSolution> {
        let mut valid_solutions = Vec::new();
        let verifier = PowVerifier::new(difficulty);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for _ in 0..attempt_count {
            // Generate random nonce
            let nonce = rand::random::<u64>();

            let solution = PowSolution {
                client_id: self.id,
                nonce,
                timestamp,
            };

            // Verify using mock PoW verifier
            if verifier.verify(&solution) {
                valid_solutions.push(solution.clone());
                self.solutions.push(solution);
                // Cap nonce to avoid overflow - use modulo for work calculation
                let capped_nonce = nonce % 1_000_000_000;
                self.total_work = self.total_work.saturating_add(capped_nonce);
            }
        }

        valid_solutions
    }
}
