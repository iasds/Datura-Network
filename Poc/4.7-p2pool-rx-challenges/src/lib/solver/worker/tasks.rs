use crate::solver::{SolverJob, SolverResult};
use super::WorkerError;
use crate::solver::utils::hash_to_difficulty;
use std::sync::{Arc,RwLock};
use tokio::sync::mpsc;
use std::time::{Instant,Duration};
use crate::SolverError;
use randomx_rs::*;


fn do_work(flags: RandomXFlag,mut job_receiver: mpsc::Receiver<SolverJob>, result_sender: mpsc::Sender<SolverResult>, cache: Arc<RwLock<RandomXCache>>,dataset: Option<Arc<RwLock<RandomXDataset>>>,seed: Arc<RwLock<[u8;32]>>) {
    let seed_guard = seed.read().unwrap();
    let mut thread_seed = *seed_guard;
    drop(seed_guard);
    let mut vm = None;
    loop {
        while let Some(job) = job_receiver.blocking_recv() {
            let pow = job.get_pow();
            update_ro_data(flags, cache, dataset, seed,&pow.seed_hash, &mut thread_seed, &mut vm);
            let seed_guard = seed.read().unwrap();
            if pow.seed_hash != *seed_guard {
                drop(seed_guard);
                let seed_guard = seed.write().unwrap();

                if pow.seed_hash != *seed_guard {
                    //no one changed it while we weren't looking
                    *seed_guard = pow.seed_hash;
                    let cache_guard = cache.write().unwrap();
                    *cache_guard = RandomXCache::new(flags, &*seed_guard);
                    new_dataset_ = if let Some(ds) = dataset {
                        let dataset_guard = dataset.write().unwrap();
                        Some(RandomXDataset::new(flags, *cache_guard.clone(),0).unwrap())
                    }
                    else {
                        None
                    };
                        
            }
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

fn update_vm(cache: RandomXCache, dataset: Option<RandomXDataset>, vm: &mut RanomXVM) {
    if dataset.is_none() {
                                vm.reinit_cache(cache);
    }
    else {

                                rxvm.reinit_dataset(dataset),
    }
}


pub fn update_ro_data(flags: RandomXFlag, cache: Arc<RwLock<RandomXCache>>,dataset: Option<Arc<RwLock<RandomXDataset>>>,seed: Arc<RwLock<[u8;32]>>, job_seed: &[u8;32],thread_seed: &mut [u8;32], vm: &mut Option<RandomXVM>) -> Result<(),RandomXError> {
            //cases:
            //global cache and dataset have been updated but not local vm => global seed is OK but
            //disagrees with thread local seed
            //
            //global cache and dataset have not been updated => global seed disagrees with local
            //seed

                    let seed_guard = seed.read().unwrap();
                    if job_seed == seed_guard && job_seed != thread_seed {
                        //case 1
                        //only need to reinit local vm
                        *thread_seed = *seed_guard;
                        drop(seed_guard);
                        let cache_guard = cache.read().unwrap();
                        let dataset_guard = if let Some(ds_guard) = dataset {
                            Some(dataset.read().unwrap().unwrap())
                        }
                        else {
                            None
                        };
                        if let Some(ref mut rxvm) = vm {
                            update_vm(*cache_guard.clone(), dataset_guard, rxvm);
                        }
                        else {
                            if mode == SolverMode::Light {
                                let cache_guard = cache.read().unwrap();
                                *vm = Some(RandomXVM::new(flags,Some(cache_guard.clone()),None).unwrap())
                            }
                            else {
                                let dataset_guard = dataset.unwrap().read().unwrap();
                                *vm = Some(RandomXVM::new(flags,None,Some(*dataset_guard.clone())))
                            }
                        };
                    }
                    else if job_seed != seed_guard && job_seed == thread_seed {
                        drop(seed_guard),
                        let seed_guard = seed.write().unwrap();

                            

                    }

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
