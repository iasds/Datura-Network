use randomx_rs::*;
use std::env;

pub struct Solver {
    vms: RandomXVM,
    mode: SolverMode,
}

pub enum SolverMode {
    Light,
    Fast,
}

pub struct DaturaPoW {
    blob: [u8;128],
    seed_hash: [u8;32],
    job_id: String,
}

impl DaturaPow {
    pub fn new(blob: [u8;128], seed_hash [u8; 32]) -> Self {
        DaturaPow {
            blob,
            seed_hash,
        }
    }
}


impl Solver {
    pub fn get_mode(&self) -> SolverMode {
        if self.dataset.is_some() {
            SolverMode::Fast
        }
        else {
            SolverMode::Light
        }
    }

    pub fn set_mode(&mut self, mode: SolverMode) {
        self.mode = mode;
    }

    pub fn solve_challenge(&mut self, challenge: DaturaPow)
}
