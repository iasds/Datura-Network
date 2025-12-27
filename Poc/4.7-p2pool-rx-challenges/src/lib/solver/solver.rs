use randomx_rs::*;

pub struct Solver {
    vms: RandomXVM,
    mode: SolverMode,
    threads: u8,
}

#[derive(Copy,Debug,Clone)]
pub enum SolverMode {
    Light,
    Fast,
}


impl Solver {
    pub fn get_mode(&self) -> SolverMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: SolverMode) {
        self.mode = mode;
    }

    //pub fn solve_challenge(&mut self, challenge: DaturaPow)
}
