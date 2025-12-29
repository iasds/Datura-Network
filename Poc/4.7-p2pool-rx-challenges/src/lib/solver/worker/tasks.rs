use crate::solver::{SolverJob, SolverResult};
use crate::solver::utils::hash_to_difficulty;
use std::sync::{Arc,RwLock};
use tokio::sync::mpsc;
use std::time::Duration;
use randomx_rs::*;

fn do_work(mut job_receiver: mpsc::Receiver<SolverJob>, result_sender: mpsc::Sender<SolverResult>,dataset: Option<Arc<RwLock<RandomXDataset>>>,cache: Option<Arc<RwLock<RandomXCache>>>,seed: Arc<RwLock<[u8;32]>>, flags: RandomXFlag) {
    let mut worst_job_duration = Duration::from_millis(0);
    let mut worst_verify_duration = Duration::from_millis(0);
    loop {
        while let Some(job) = job_receiver.blocking_recv() {
            match job {
                Verify((pow,solution_candidate)) => {
                    let difficulty = hash_to_difficulty(solution_candidate.as_slice().into());
                    if difficulty < pow.target {
                        result_sender.blocking_send(SolverResult::Invalid((pow,WorkerError::LowDifficultyShare.into())));
                        continue;
                    }

                    if let Err(err) = update_ro_data(dataset, cache, seed,flags, &pow.seed_hash) {
                        result_sender.blocking_send(SolverError::RandomXError(err));
                        continue;
                    }
                    let work_start = Instant::now();
                    let vm = if cache.is_some() {
                        let cache = cache.unwrap().read().unwrap();
                        let dataset = dataset.unwrap().read().unwrap();
                        match RandomXVM::new(flags, Some(*cache), Some(*dataset)) {
                            Ok(vm) => vm,
                            Err(some_err) => {
                            result_sender.blocking_send(SolverError::RandomXError(some_err));
                            continue;
                            }
                        }
                    } else if dataset.is_some() {
                        let dataset = dataset.unwrap().read().unwrap();
                        match RandomXVM::new(flags,None,Some(*dataset)) {
                            Ok(vm) => vm,
                            Err(some_err) => {

                            result_sender.blocking_send(SolverError::RandomXError(some_err));
                            continue;
                            }
                        }
                    } else {
                        panic!("I have neither cache nor dataset!");
                    };

                    match vm.calculate_hash(&pow.blob) {
                        Ok(solution) => {
                            if solution_candidate != solution {
                                result_sender.blocking_send(SolverResult::Invalid((pow,WorkerError::InvalidShare.into())));
                                continue;
                            }
                            result_sender.blocking_send(SolverResult::Valid(pow));
                            let work_time = work_start.elapsed();
                            if work_time > worst_verify_duration {
                                worst_verify_duration = work_time;
                            }
                        },
                        Err(some_err) => {
                            result_sender.blocking_send(SolverResult::Error(some_err.into()));
                        }
                    }
                }
                Solve((mut pow,alloted_time)) => {
                    let work_start = Instant::now();
                    if let Err(err) = update_ro_data(dataset, cache, seed, flags,&pow.seed_hash) {
                        result_sender.blocking_send(some_err.into());
                        continue;
                    }
                    let mut best_solution = (Vec::new(),1u64);
                    while work_start.elapsed() < alloted_time {
                        challenge.new_nonce();
                        let solution = vm.calculate_hash(&pow.blob)?;
                        let difficulty = hash_to_difficulty(&solution_candidate.as_slice());
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


pub fn update_ro_data(dataset: Option<Arc<RwLock<RandomXDataset>>>,cache: Option<Arc<RwLock<RandomXCache>>>,seed: Arc<RwLock<[u8;32]>>, flags: RandomXFlag,job_seed: &[u8;32]) -> Result<(),RandomXError> {
                    let curseed = seed.read().unwrap();
                    if *curseed != *job_seed {
                        let mut newseed = seed.write().unwrap();
                        *newseed = *job_seed;
                        if let Some(ref old_cache) = cache {
                            //reinit cache
                            let new_cache = RandomXCache::new(flags, &*newseed)?;
                            let mut cache = old_cache.write().unwrap();
                            *cache = new_cache;
                        }
                        if let Some(ref old_dataset) = dataset {
                            if let Some(rwl_cache) = cache {
                                let r_cache = rwl_cache.read().unwrap();
                                let new_dataset = RandomXDataset::new(flags,r_cache.clone(),0)?;
                                let mut dataset = old_dataset.write().unwrap();
                                *dataset = new_dataset;
                            }
                            else {
                                panic!("can't initialize a dataset without a cache");
                            }
                        }
                    }
                    Ok(())

}

