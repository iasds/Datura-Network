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

pub struct DaturaPow {
    blob: [u8;128],
    seed_hash: [u8;32],
    job_id: String,
}

impl TryFrom<ServerReply> for DaturaPow {
    fn try_from(workOrker: ServerReply) -> Result<> {

    }

}

impl DaturaPow {
    pub fn new(blob: [u8:128], seed_hash [u8: 32], job_id: String) -> Self {
        DaturaPow {
            blob,
            seed_hash,
            job_id,
        }
    }
    pub fn random() -> Self {
        //generate really random challenge
        DaturaPow {
            blob: [0u8;128],
            seed_hash: [0u8;32],
            job_id: "0".to_string(),
        }
    }
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
