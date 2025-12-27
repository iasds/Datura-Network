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

    pub fn solve_challenge(&mut self, JobData
}
