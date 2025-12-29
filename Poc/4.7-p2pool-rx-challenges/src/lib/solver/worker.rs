use super::solver::{SolverMode,SolverJob, DaturaPow};
use std::sync::{Arc,RwLock};
use tokio::sync::mpsc;
use randomx_rs::*;
use std::thread::JoinHandle;
use thiserror::Error;
use std::time::Instant;

#[derive(Error,Debug)]
pub enum WorkerError {
    #[error("invalid configuration, cache or dataset required")]
    InvalidConfiguration,
    #[error("error intiializing randomXVM")]
    RandomXError(#[from]RandomXError),
    #[error("invalid share submitted")]
    InvalidShare,
    #[error("hash solved is too low difficulty")]
    LowDifficultyShare,
}

pub struct Worker {
    mode: SolverMode,
    flags: randomXFlag,
    dataset: Option<Arc<RwLock<RandomXDataSet>>>,
    cache: Option<Arc<RwLock<RandomXCache>>>,
    pub feed_route: mpsc::Sender<SolverJob>,
    pub job_results: mpsc::Receiver<SolverResult>,
    pub handle:JoinHandle,
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

fn update_ro_data(dataset: Option<Arc<RwLock<RandomXDataSet>>>,cache: Option<Arc<RwLock<RandomXCache>>>,seed: Arc<RwLock<[u8;32]>>) -> Result<(),RandomXError> {
                    let curseed = seed.read().unwrap();
                    if *curseed != seed_hash {
                        let newseed = seed.write().unwrap();
                        *newseed = seed_hash;
                        if let Some(old_cache) = cache {
                            //reinit cache
                            new_cache = RandomXCache::new(flags, &seed)?;
                            let cache = old_cache.write().unwrap();
                            *cache = new_cache;
                        }
                        if let Some(old_dataset) = dataset {
                            let cache = cache.read().unwrap();
                            let new_dataset = match RandomXDataset::new(flags,*cache,0)?;
                            };
                            let dataset = dataset.write().unwrap();
                            *dataset = new_dataset;
                        }
                    }
                    Ok(())

}


fn do_work(mut job_receiver: mpsc::Receiver<SolverJob>, result_sender: mpsc::Sender<SolverResult>,dataset: Option<Arc<RwLock<RandomXDataSet>>>,cache: Option<Arc<RwLock<RandomXCache>>>,seed: Arc<RwLock<[u8;32]>>, flags: RandomXFlag) {
    let mut worst_job_duration = Instant::Duration::from_millis(0);
    loop {
        while let Ok(Some(job)) = job_receiver.blocking_recv() {
            match SolverJob {
                Verify((pow,solution_candidate)) => {
                    let difficulty = hash_to_difficulty(&pow.blob);
                    if difficulty < pow.target {
                        result_sender.send(SolverResult::Ivalid((pow,WorkerError::LowDifficultyShare.into())));
                        continue;
                    }

                    if let Some(err) = update_ro_data(dataset, cache, seed) {
                        result_sender.send(some_err.into());
                        continue;
                    }
                    let work_start = Instant::now();
                    let vm = if cache.is_some() {
                        let cache = cache.unwrap().read().unwrap();
                        match RandomXVM::new(flags, Some(*cache), dataset) {
                            Ok(vm) => vm,
                            Err(some_err) => {
                            result_sender.send(some_err.into());
                            contnue;
                            }
                        }
                    } else if dataset.is_some() {
                        let dataset = dataset.unwrap().read().unwrap();
                        match RandomXVM::new(flags,None,Some(*dataset)) {
                            Ok(vm) => vm,
                            Err(some_err) => {

                            result_sender.send(some_err.into());
                            contnue;
                            }
                        }
                    } else {
                        panic!("I have neither cache nor dataset!");
                    };

                    match vm.calculate_hash(&pow.blob) {
                        Ok(solution) => {
                            if solution_candidate != solution {
                                result_sender.send(SolverResult::Invalid((pow,WorkerError::InvalidShare.into())));
                                continue;
                            }
                            result_sender.send(SolverResult::Valid(pow));
                        },
                        Err(some_err) => {
                            result_sender.send(SolverResult::Error(some_err.into()));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

impl Worker {
    pub fn new(dataset: Option<Arc<RwLock<RandomXDataSet>>>,cache: Option<Arc<RwLock<RandomXCache>>>, seed: Arc<RwLock<[u8;32]>>) -> Result<Self,WorkerError> {
        let (feed_route, job_receiver) = mpsc::channel(1);
        let (result_sender, job_results) = mpsc::channel(1);
        let mode = if dataset.is_some() {
            SolverMode::Fast
        }
        else if cache.is_some() {
            SolverMode::Light
        }
        else {
            return Err(WorkerError::InvalidConfiguration);
        };
        let flags = RandomXFlag::get_recommended_flags() | (
            if mode == dataset.is_some {
                RandomXFlag::FLAG_LARGE_PAGES | RandomXFlag::FLAG_FULL_MEM
            }
            else {
                RandomXFlag::empty()
            });

        Ok(Worker {
            mode,
            feed_route,
            flags,
            dataset,
            cache,
            feed_route,
            job_results,
            seed,
            handle,
        })
    }
}
