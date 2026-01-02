use super::*;
use crate::solver::SolverJob;
use crate::solver::solver::{SharedCache, SharedDataset};
use crate::solver::utils::*;
use crate::solver::*;
use randomx_rs::*;
use std::cell::UnsafeCell;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::sync::mpsc;

pub struct Worker {
    flags: RandomXFlag,
    state: WorkerState,
    job_channel: mpsc::Receiver<SolverJob>,
    job_results: mpsc::Sender<SolverResult>,
    seed: Arc<RwLock<[u8; 32]>>,
    thread_seed: [u8; 32],
    pub available: Arc<AtomicBool>,
}
pub fn get_pow(job: &SolverJob) -> &DaturaPow {
    match job {
        SolverJob::Verify((pow, _, _)) | SolverJob::Solve((pow, _)) => pow,
    }
}

#[derive(Debug, Clone)]
pub enum WorkerState {
    Light { cache: Arc<SharedCache> },
    Fast { dataset: Arc<SharedDataset> },
}

impl WorkerState {
    pub fn update(
        &mut self,
        flags: RandomXFlag,
        seed: Arc<RwLock<[u8; 32]>>,
        job_seed: &[u8; 32],
        thread_seed: &mut [u8; 32],
        vm: &mut Option<RandomXVM>,
    ) {
        let mut seed_guard = seed.blocking_write();
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
            match self {
                WorkerState::Light { cache } => {
                    let _cache_guard = cache.cache.blocking_read();
                    match vm {
                        Some(rxvm) => {
                            rxvm.reinit_cache(cache.get()).unwrap();
                        }
                        None => {
                            *vm = Some(RandomXVM::new(flags, Some(cache.get()), None).unwrap());
                        }
                    }
                }
                WorkerState::Fast { dataset } => {
                    let _ds_guard = dataset.dataset.blocking_read();
                    match vm {
                        Some(rxvm) => {
                            rxvm.reinit_dataset(dataset.get()).unwrap();
                        }
                        None => {
                            *vm = Some(RandomXVM::new(flags, None, Some(dataset.get())).unwrap());
                        }
                    }
                }
            }
        } else if *job_seed != *seed_guard {
            if *seed_guard == *job_seed {
                //someone already finished updating the cache and dataset
                //while we were waiting on seed guard
                self.update(flags, seed.clone(), job_seed, thread_seed, vm);
            } else {
                //we need to update the cache/dataset ourselves then reinit our localvm
                *seed_guard = *job_seed;
                match self {
                    WorkerState::Light { cache, .. } => {
                        let mut cache_guard = cache.cache.blocking_write();
                        *cache_guard = UnsafeCell::new(RandomXCache::new(flags, job_seed).unwrap());
                    }
                    WorkerState::Fast { dataset, .. } => {
                        let cache = RandomXCache::new(flags, job_seed).unwrap();
                        let mut ds_guard = dataset.dataset.blocking_write();
                        *ds_guard = UnsafeCell::new(RandomXDataset::new(flags, cache, 0).unwrap());
                    }
                }
                //now let's run again to update our vm state and let others work
                drop(seed_guard);
                self.update(flags, seed.clone(), job_seed, thread_seed, vm);
            }
        }
    }
}

impl Worker {
    pub fn new(
        flags: RandomXFlag,
        state: WorkerState,
        seed: Arc<RwLock<[u8; 32]>>,
        job_channel: mpsc::Receiver<SolverJob>,
        job_results: mpsc::Sender<SolverResult>,
        available: Arc<AtomicBool>,
    ) -> Result<Self, WorkerError> {
        Ok(Worker {
            flags,
            state,
            job_channel,
            job_results,
            seed,
            thread_seed: [0u8; 32],
            available,
        })
    }

    pub fn start(&mut self) {
        let mut vm = None;
        while let Some(job) = self.job_channel.blocking_recv() {
            self.available.store(false, Ordering::Release);
            let pow = get_pow(&job);
            self.state.update(
                self.flags,
                self.seed.clone(),
                &pow.seed_hash,
                &mut self.thread_seed,
                &mut vm,
            );
            match job {
                SolverJob::Verify((pow, solution_candidate, _)) => {
                    let difficulty =
                        hash_to_difficulty(solution_candidate.as_slice().try_into().unwrap());
                    let target_diff = get_difficulty(&pow.target).unwrap();
                    if difficulty > target_diff {
                        println!("not at target diff (worker drop");
                        self.job_results.blocking_send(SolverResult::Invalid((
                            pow,
                            WorkerError::LowDifficultyShare.into(),
                        ))).unwrap();
                        continue;
                    }
                    if let Some(ref rxvm) = vm {
                        let solution = rxvm.calculate_hash(&pow.blob).unwrap();
                        if solution != solution_candidate {
                            println!("bad solution");
                            self.job_results.blocking_send(SolverResult::Error(
                                SolverError::DaturaPowInvalidResponse,
                            )).unwrap();
                        } else {
                            println!("valid result {} for target at {}", difficulty, target_diff);
                            self.job_results
                                .blocking_send(SolverResult::Valid((pow, solution))).unwrap();
                        }
                    } else {
                        panic!("no vm to run!");
                    }
                }
                SolverJob::Solve((mut pow, end_date)) => {
                    let mut best_solution = (DaturaPow::random([0u8; 32]), Vec::new(), 0u64);
                    let target_diff = get_difficulty(&pow.target).unwrap();
                    while Instant::now() < end_date {
                        pow.new_nonce();
                        let result = if let Some(ref rxvm) = vm {
                            rxvm.calculate_hash(&pow.blob).unwrap()
                        } else {
                            panic!("no vm to run!");
                        };
                        let difficulty = hash_to_difficulty(result.as_slice().try_into().unwrap());
                        if difficulty <= target_diff {
                            println!(
                                "found a solution with diff {} for target {}",
                                difficulty, target_diff
                            );
                            best_solution = (pow.clone(), result.clone(), difficulty);
                            //this thread has done its job and found a result, returning it ASAP
                            //while others might continue to search
                            break;
                        }
                    }
                    if best_solution.2 >= target_diff {
                        self.job_results.blocking_send(SolverResult::Solved((
                            best_solution.0,
                            best_solution.1,
                        ))).unwrap();
                    }
                }
            }
            self.available.store(true, Ordering::Release);
        }
    }
}
