use crate::solver::SolverJob;
use std::cell::UnsafeCell;
use std::time::Instant;
use std::sync::{Arc,RwLock, atomic::{AtomicBool, Ordering}};
use tokio::sync::mpsc;
use randomx_rs::*;
use super::*;
use crate::solver::utils::*;
use crate::solver::*;
use crate::solver::solver::{SharedDataset, SharedCache};

pub struct Worker {
    flags: RandomXFlag,
    state: WorkerState,
    job_channel: mpsc::Receiver<SolverJob>,
    job_results: mpsc::Sender<SolverResult>,
    seed: Arc<RwLock<[u8;32]>>,
    thread_seed: [u8;32],
    pub available: AtomicBool,
}
    pub fn get_pow(job: &SolverJob) -> &DaturaPow {
        match job {
            SolverJob::Verify((pow,_,_))|SolverJob::Solve((pow,_)) => &pow,
        }
    }


#[derive(Debug,Clone)]
pub enum WorkerState {
    Light{
        cache: Arc<SharedCache>,
    },
    Fast{
        dataset: Arc<SharedDataset>,
    }
}

impl WorkerState {
    pub fn update(&mut self,flags: RandomXFlag,seed: Arc<RwLock<[u8;32]>>,job_seed: &[u8;32], thread_seed: &mut [u8;32], vm: &mut Option<RandomXVM>) {
        let seed_guard = seed.read().unwrap();
            //cases:
            //global cache and dataset have been updated but not local vm => global seed is OK but
            //disagrees with thread local seed
            //
            //global cache and dataset have not been updated => global seed disagrees with local
            //seed

                    if *job_seed == *seed_guard && job_seed != thread_seed {
                        //case 1
                        //only need to reinit local vm
                        *thread_seed = *seed_guard;
                        drop(seed_guard);
                        match self {
                            WorkerState::Light {cache} => {
                                let _cache_guard = cache.cache.read().unwrap();
                                match vm {
                                    Some(rxvm) => {
                                        rxvm.reinit_cache(cache.get()).unwrap();
                                    }
                                    None => {
                                        *vm = Some(RandomXVM::new(flags, Some(cache.get()),None).unwrap());
                                    }
                                }
                            }
                            WorkerState::Fast { dataset} => {
                                let _ds_guard = dataset.dataset.read().unwrap();
                                match vm {
                                    Some(rxvm) => {
                                        rxvm.reinit_dataset(dataset.get()).unwrap();
                                    }
                                    None => {
                                        *vm = Some(RandomXVM::new(flags,None, Some(dataset.get())).unwrap());
                                    }
                                }
                            }
                        }
                    }
                    else if *job_seed != *seed_guard && job_seed == thread_seed {
                        drop(seed_guard);
                        let mut seed_guard = seed.write().unwrap();
                        if *seed_guard == *job_seed {
                            //someone already finished updating the cache and dataset
                            //while we were waiting on seed guard
                            self.update(flags, seed.clone(), job_seed, thread_seed, vm);
                        }
                        else {
                            //we need to update the cache/dataset ourselves then reinit our localvm
                            *seed_guard = *job_seed;
                            match self {
                                WorkerState::Light {cache,..} => {
                                    let mut cache_guard = cache.cache.write().unwrap();
                                    *cache_guard = UnsafeCell::new(RandomXCache::new(flags, job_seed).unwrap());
                                }
                                WorkerState::Fast {dataset,..} => {
                                    let cache = RandomXCache::new(flags, job_seed).unwrap();
                                    let mut ds_guard = dataset.dataset.write().unwrap();
                                    *ds_guard = UnsafeCell::new(RandomXDataset::new(flags, cache,0).unwrap());
                                }
                            }
                            //now let's run again to update our vm state and let others work
                            self.update(flags, seed.clone(), job_seed, thread_seed,vm);
                        }
                    }
            }

}


impl Worker {
    pub fn new(flags: RandomXFlag, state: WorkerState,  seed: Arc<RwLock<[u8;32]>>, job_channel: mpsc::Receiver<SolverJob>, job_results: mpsc::Sender<SolverResult>, available: AtomicBool) -> Result<Self,WorkerError> {
        Ok(Worker {
            flags,
            state,
            job_channel,
            job_results,
            seed,
            thread_seed: [0u8;32],
            available,
        })
    }

    pub fn start(&mut self) {
        let mut vm = None;
        while let Some(job) = self.job_channel.blocking_recv() {
            self.available.store(false, Ordering::Release);
            let pow = get_pow(&job);
            self.state.update(self.flags, self.seed.clone(), &pow.seed_hash, &mut self.thread_seed, &mut vm);
            match job {
                SolverJob::Verify((pow,solution_candidate,_)) => {
                    let difficulty = hash_to_difficulty(solution_candidate.as_slice().try_into().unwrap());
                    if difficulty < pow.target {
                        self.job_results.blocking_send(SolverResult::Invalid((pow,WorkerError::LowDifficultyShare.into())));
                        continue;
                    }
                    if let Some(ref rxvm) = vm {
                        let solution = rxvm.calculate_hash(&pow.blob).unwrap();
                        if solution != solution_candidate {
                            self.job_results.blocking_send(SolverResult::Error(SolverError::DaturaPowInvalidResponse));
                        }
                    }
                    else {
                        panic!("no vm to run!");
                    }
                }
                SolverJob::Solve((mut pow,end_date)) => {
                    let mut best_solution = (Vec::new(),1u64);
                    while Instant::now() < end_date {
                        pow.new_nonce();
                        let solution = if let Some(ref rxvm) = vm {
                            rxvm.calculate_hash(&pow.blob).unwrap()
                        }
                        else {
                            panic!("no vm to run!");
                        };
                        let difficulty = hash_to_difficulty(solution.as_slice().try_into().unwrap());
                        if difficulty > pow.target && difficulty > best_solution.1 {
                            best_solution = (solution,difficulty);
                        }
                    }
                    if best_solution.1 >= pow.target {
                        self.job_results.blocking_send(SolverResult::Solved((pow,best_solution.0)));
                    }
                }
            }
            self.available.store(true, Ordering::Release);
        }
    }
}
