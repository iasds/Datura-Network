use crate::solver::{SolverMode,SolverJob};
use std::sync::{Arc,RwLock};
use tokio::sync::mpsc;
use randomx_rs::*;
use super::*;
use crate::solver::utils::*;
use crate::solver::*;
use tokio::CancellationToken

pub struct Worker {
    flags: RandomXFlag,
    state: WorkerState,
    job_channel: mpsc::Receiver<SolverJob>,
    job_results: mpsc::Sender<SolverResult>,
    seed: Arc<RwLock<[u8;32]>>,
    thread_seed: [u8;32],
}

#[derive(Debug,Clone)]
enum WorkerState {
    Light {
        cache: Arc<RwLock<RandomXCache>>,
        vm: Option<RandomXVM>,
    }
    Fast {
        dataset: Arc<RwLock<RandomXDataset>>,
        vm: Option<RandomXVM>,
    }
}

impl WorkerState {
    pub fn get_vm(&self) -> &RandomXVM {
        match self {
            Light{vm,..}|Fast{vm,..} => vm,
        }
    }
    pub fn update(&mut self,flags: randomXFlag,seed: Arc<RwLock<[u8;32]>>,job_seed: &[u8;32], thread_seed: &mut [u8;32]) {
        let seed_guard = seed.read().unwrap();
            //cases:
            //global cache and dataset have been updated but not local vm => global seed is OK but
            //disagrees with thread local seed
            //
            //global cache and dataset have not been updated => global seed disagrees with local
            //seed

                    if job_seed == seed_guard && job_seed != &self.thread_seed {
                        //case 1
                        //only need to reinit local vm
                        *thread_seed = *seed_guard;
                        drop(seed_guard);
                        match self {
                            Light {cache, vm, ..} => {
                                let cache_guard = cache.read().unwrap();
                                match vm {
                                    Some(rxvm) => {
                                        rxvm.reinit_cache(*cache_guard.clone()).unwrap();
                                    }
                                    None => {
                                        vm = Some(RandomXVM::new(flags, Some(*cache_guard.clone()),None));
                                    }
                                }
                            }
                            Fast { dataset, vm, ..} => {
                                let ds_guard = dataset.read().unwrap();
                                match vm {
                                    Some(rxvm) => {
                                        vm.reiit_dataset(*ds_guard.clone()).unwrap();
                                    }
                                    None => {
                                        vm = Some(RandomXVM::new(flags,None, Some(*ds_guard.clone())));
                                    }
                                }
                            }
                        }
                    }
                    else if job_seed != seed_guard && job_seed == thread_seed {
                        drop(seed_guard),
                        let seed_guard = seed.write().unwrap();
                        if *seed_guard == job_seed {
                            //someone already finished updating the cache and dataset
                            //while we were waiting on seed guard
                            self.update_state(flags, seed, job_seed, thread_seed);
                        }
                        else {
                            //we need to update the cache/dataset ourselves then reinit our localvm
                            *seed_guard = job_seed;
                            match self {
                                Light {cache,..} => {
                                    let cache_guard = cache.write().unwrap();
                                    *cache_guard = RandomXCache:new(flags, &job_seed).unwrap();
                                }
                                Fast {dataset,..} => {
                                    let cache = RandomXCache:new(flags, &job_seed).unwrap();
                                    let ds_guard = dataset.write().unwrap();
                                    *ds_guard = RandomXDataset::new(flags, cache,0).unwrap();
                                }
                            }
                            //now let's run again to update our vm state and let others work
                            self.update_state(flags, seed, job_seed, thread_seed);
                        }
                    }
            }

}


impl Worker {
    pub fn new(flags: RandomXFlag, state: WorkerState, mode: SolverMode, seed: Arc<RwLock<[u8;32]>>, job_channel: mpsc::Receiver<SolverJob>, job_results: mpsc::Sender<SolverResult>) -> Result<Self,WorkerError> {
        Ok(Worker {
            flags,
            state,
            job_channel,
            job_results,
            seed,
            thread_seed: [0u8;32],
        })
    }

    pub fn start(&mut self) {
        while let Some(job) = job_receiver.blocking_recv() {
            let pow = job.get_pow();
            self.state.update(self.flags, self.seed, &pow.seed, self.thread_seed);
            match job {
                SolverJob::Verify((pow,solution_candidate)) => {
                    let difficulty = hash_to_difficulty(solution_candidate.as_slice().try_into().unwrap());
                    if difficulty < pow.target {
                        result_sender.blocking_send(SolverResult::Invalid((pow,WorkerError::LowDifficultyShare.into())));
                        continue;
                    }
                    let vm = self.state.get_vm();
                    let solution = vm.calculate_hash(&pow.bob).unwrap();
                    if solution != solution_candidate {
                        result_sender.blocking_send(SolverResult::Error(SolverError::DaturaPowInvalidResponse));
                    }

                }
                SolverJob::Solve((mut pow,alloted_time)) => {
                    let work_start = Instant::now();
                    let mut best_solution = (Vec::new(),1u64);
                    let vm = self.state.get_vm();
                    while work_start.elapsed() < alloted_time {
                        pow.new_nonce();
                        let solution = vm.calculate_hash(&pow.blob).unwrap();
                        let difficulty = hash_to_difficulty(solution.as_slice().try_into().unwrap());
                        if difficulty > pow.target && difficulty > best_solution.1 {
                            best_solution = (solution,difficulty);
                        }
                    }
                    if best_solution.1 >= pow.target {
                        result_sender.blocking_send(SolverResult::Solved((pow,best_solution.0)));
                    }
                }
            }
        }
    }
}
