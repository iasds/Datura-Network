use rand::Rng;

/// Simple hash function for PoW mock (murmur-like)
pub fn mock_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0;
    for byte in data {
        hash = hash.wrapping_mul(31).wrapping_add(*byte as u64);
    }
    hash
}

/// Count leading zeros in binary representation
pub fn count_leading_zeros(val: u64) -> u32 {
    val.leading_zeros()
}

/// Represents a PoW solution attempt from a client
#[derive(Debug, Clone)]
pub struct PowSolution {
    pub client_id: u64,
    pub nonce: u64,
    pub timestamp: u64,
    pub difficulty_target: u64,
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

    /// Calculate work difficulty from a solution
    pub fn calculate_work_difficulty(&self, solution: &PowSolution) -> u32 {
        let value = (solution.nonce % 256) as u32;
        // Count how many bits we satisfy
        if value < 16 { 4 } else if value < 32 { 3 } else if value < 64 { 2 } else if value < 128 { 1 } else { 0 }
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
        num_attempts: u64,
        difficulty: u32,
        mut _rng: impl Rng,
    ) -> Vec<PowSolution> {
        let mut valid_solutions = Vec::new();
        let verifier = PowVerifier::new(difficulty);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for _ in 0..num_attempts {
            // Generate random nonce
            let nonce = rand::random::<u64>();

            let solution = PowSolution {
                client_id: self.id,
                nonce,
                timestamp,
                difficulty_target: difficulty as u64,
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
