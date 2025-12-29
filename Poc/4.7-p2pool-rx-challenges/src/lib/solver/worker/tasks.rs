use crate::solver::{SolverJob, SolverResult};
use super::WorkerError;
use crate::solver::utils::hash_to_difficulty;
use std::sync::{Arc,RwLock};
use tokio::sync::mpsc;
use std::time::{Instant,Duration};
use crate::SolverError;
use randomx_rs::*;


fn do_work(mut job_receiver: mpsc::Receiver<SolverJob>, result_sender: mpsc::Sender<SolverResult>, vm: &RandomXVM) {
    let mut worst_job_duration = Duration::from_millis(0);
    let mut worst_verify_duration = Duration::from_millis(0);
    loop {
        while let Some(job) = job_receiver.blocking_recv() {
            let (pow,solution_candidate, time_allotment) = match job {
                SolverJob::Verify((pow,solution_candidate)) => {
                    let difficulty = hash_to_difficulty(solution_candidate.as_slice().try_into().unwrap());
                    if difficulty < pow.target {
                        result_sender.blocking_send(SolverResult::Invalid((pow,WorkerError::LowDifficultyShare.into())));
                        continue;
                    }

                    if let Err(err) = update_ro_data(dataset, cache, seed,flags, &pow.seed_hash) {
                        result_sender.blocking_send(SolverResult::Error(SolverError::RandomXError(err)));
                        continue;
                    }
                    (Some(pow),Some(solution_candidate),None)
                }
                SolverJob::Solve((mut pow,alloted_time)) => {
                    if let Err(err) = update_ro_data(dataset, cache, seed, flags,&pow.seed_hash) {
                        result_sender.blocking_send(SolverResult::Error(SolverError::RandomXError(err)));
                        continue;
                    }
                    (Some(pow),None,Some(alloted_time))

                }
            };
                    let vm = if cache.is_some() {
                        let cache = cache.unwrap().read().unwrap();
                        let dataset = dataset.unwrap().read().unwrap();
                        match RandomXVM::new(flags, Some(*cache), Some(*dataset)) {
                            Ok(vm) => vm,
                            Err(some_err) => {
                            result_sender.blocking_send(SolverResult::Error(SolverError::RandomXError(some_err)));
                            continue;
                            }
                        }
                    } else if let Some(set) = dataset {
                        let rl_dataset = set.read().unwrap();
                        match RandomXVM::new(flags,None,Some(*rl_dataset)) {
                            Ok(vm) => vm,
                            Err(some_err) => {

                            result_sender.blocking_send(SolverResult::Error(SolverError::RandomXError(some_err)));
                            continue;
                            }
                        }
                    } else {
                        panic!("I have neither cache nor dataset!");
                    };


                    let work_start = Instant::now();
                    match (pow,solution_candidate,time_allotment) {
                        (Some(mut pow),None,Some(alloted_time)) => {
                                let mut best_solution = (Vec::new(),1u64);
                                while work_start.elapsed() < alloted_time {
                                    pow.new_nonce();
                                    let solution = vm.calculate_hash(&pow.blob).unwrap();
                                    let difficulty = hash_to_difficulty(solution.as_slice().try_into().unwrap());
                                    if difficulty > pow.target && difficulty > best_solution.1 {
                                        best_solution = (solution,difficulty);
                                    }
                                }
                                let total = work_start.elapsed();
                                if total > worst_job_duration {
                                    worst_job_duration = total;
                                }
                                if best_solution.1 >= pow.target {
                                    result_sender.blocking_send(SolverResult::Solved((pow,best_solution.0)));
                                }
                        }
                        (Some(pow),Some(solution_candidate),None) => {


                                match vm.calculate_hash(&pow.blob) {
                                    Ok(solution) => {
                                        if solution_candidate != solution {
                                            result_sender.blocking_send(SolverResult::Invalid((pow,WorkerError::InvalidShare.into())));
                                            continue;
                                        }
                                        result_sender.blocking_send(SolverResult::Valid((pow,solution_candidate)));
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
                        _ => panic!("misparsing of task"),

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

