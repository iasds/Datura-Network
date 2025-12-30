use crate::solver::{SolverMode,SolverJob};
use std::sync::{Arc,RwLock};
use tokio::sync::mpsc;
use randomx_rs::*;
use super::*;
use crate::solver::utils::*;
use crate::solver::*;

pub struct Worker {
    mode: SolverMode,
    flags: RandomXFlag,
    dataset: Option<Arc<RwLock<RandomXDataset>>>,
    cache: Option<Arc<RwLock<RandomXCache>>>,
    pub feed_route: mpsc::Sender<SolverJob>,
    job_results: mpsc::Sender<SolverResult>,
    worker_thread_receiver: mpsc::Receiver<SolverJob>,
    seed: Arc<RwLock<[u8;32]>>,
}



impl Worker {
    pub fn new(dataset: Option<Arc<RwLock<RandomXDataset>>>,cache: Option<Arc<RwLock<RandomXCache>>>, seed: Arc<RwLock<[u8;32]>>, job_results: mpsc::Sender<SolverResult>, _vm: &RandomXVM) -> Result<Self,WorkerError> {
        let (feed_route, worker_thread_receiver) = mpsc::channel(1);
        let mode = if dataset.is_some() {
            SolverMode::Fast
        }
        else if cache.is_some() {
            SolverMode::Light
        }
        else {
            return Err(WorkerError::InvalidConfiguration);
        };
        let flags = get_flags(mode);

        Ok(Worker {
            mode,
            feed_route,
            flags,
            worker_thread_receiver,
            dataset,
            cache,
            job_results,
            seed,
        })
    }
}
