use randomx_rs::*;
use crate::SolverError;
use crate::DaturaPow;
use super::utils::check_hash;


///Solver for challenge completion and verification
///threadnumber serves to force a limit on workers, set to 0 for automatically
///use as many cores as available
pub struct Solver {
    vm: RandomXVM,
    mode: SolverMode,
    threads: u8,
    flags: RandomXFlag,
    seed: [u8;32],
}

#[derive(Copy,Debug,Clone,PartialEq)]
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

    fn prepare_vm(&mut self,challenge: &DaturaPow) -> Result<(),SolverError>{
        if self.seed != challenge.seed_hash {
            let cache = RandomXCache::new(self.flags, &challenge.seed_hash)?;
            if self.mode == SolverMode::Light {
                let cache = RandomXCache::new(self.flags, &challenge.seed_hash)?;
                self.vm.reinit_cache(cache)?;

            }
            else {
                let dataset = RandomXDataset::new(self.flags, cache,0)?;
                self.vm.reinit_dataset(dataset)?;
            }
            self.seed = challenge.seed_hash.clone();
        }
        Ok(())
    }

    pub fn check_answer(&mut self, challenge: &DaturaPow) -> Result<(),SolverError> {
        self.prepare_vm(challenge)?;
        let solution = self.vm.calculate_hash(&challenge.blob)?;
        if check_hash(solution.as_slice().try_into().unwrap(), challenge.target) {
            return Ok(());
        }
        Err(SolverError::DaturaPowInvalidResponse)
    }
    
    pub fn solve_challenge(&mut self,mut challenge: DaturaPow) -> Result<(DaturaPow,Vec<u8>), SolverError> {
        self.prepare_vm(&challenge)?;
        let mut prev_nonce = challenge.nonce;
        loop {
            let curnonce = challenge.nonce;
            if curnonce < prev_nonce { //we wrapped over
                return Err(SolverError::DaturaPowExhaustedSearchSpace);
            }
            prev_nonce = curnonce;
            let solution = self.vm.calculate_hash(&challenge.blob)?;
            if check_hash(solution.as_slice().try_into().unwrap(), challenge.target) {
                return Ok((challenge,solution));
            }
            challenge.next_nonce();
        }
    }
    pub fn new(mode: SolverMode, threads: u8) -> Result<Self,SolverError> {
        let flags = RandomXFlag::get_recommended_flags() | (
            if mode == SolverMode::Fast {
                RandomXFlag::FLAG_LARGE_PAGES | RandomXFlag::FLAG_FULL_MEM
            }
            else {
                RandomXFlag::empty()
            });
        let cache = RandomXCache::new(flags,&[0u8;32])?;
        let dataset = if mode==SolverMode::Fast {
            Some(RandomXDataset::new(flags, cache.clone(),0)?)
        }
        else {
            None
        };
        Ok(Solver {
            vm: RandomXVM::new(flags, Some(cache), dataset)?,
            mode,
            flags,
            seed: [0u8;32],
        threads})
    }
}
