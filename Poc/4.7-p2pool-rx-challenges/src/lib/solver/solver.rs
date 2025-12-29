use randomx_rs::*;
use crate::SolverError;
use crate::solver::worker::*;
use tokio::sync::mpsc;
use crate::consts::*;
use std::sync::{Arc,RwLock};
use super::utils::*;
use std::cmp::min;
use super::models::*;




///Solver for challenge completion and verification
///threadnumber serves to force a limit on workers, set to 0 for automatically
///use as many cores as available
pub struct Solver {

    ///define the solver mode: in light mode we only use one or more light workers, in
    ///fast mode we use one light worker for verifications (or hashing if it has nothing else to
    ///do) and the others for everything else
    mode: SolverMode,

    ///if configured to run into more than 1 thread then other workers will be added
    ///those workers can be light workers by default, or fast depending on the mode used
    solvers: Vec<Worker>,

    ///We always have at least 1 light mode worker
    verifier: Worker,
    ///Used to send SolverJob such as PoW to the solver
    pub feed_route: mpsc::Sender<SolverJob>,

    ///Used by the solver tasks to receive new solverjobs
    receiver: mpsc::Receiver<SolverJob>,

    ///Used by the workers to send back results
    worker_results: mpsc::Receiver<SolverResult>,

    ///Used by the solver to send back results to the outside
    pub solver_output: mpsc::Sender<SolverResult>,
}

#[derive(Copy,Debug,Clone,PartialEq)]
pub enum SolverMode {
    Light,
    Fast,
}

impl Solver {
    ///If creating with multiple threads one will be a dedicated verification thread and priorize
    ///verification work
    pub fn new(mode: SolverMode, mut nb_threads: usize, solver_output: mpsc::Sender<SolverResult>) -> Result<Self,SolverError> {
        nb_threads = min(nb_threads,num_cpus::get());
        let flags = get_flags(mode);
        let cache = Arc::new(RwLock::new(RandomXCache::new(flags,&[0u8;32])?)); //always at least one worker thread
        let dataset = if mode==SolverMode::Fast {
            let locked_cache = cache.read().unwrap();
            Some(Arc::new(RwLock::new(RandomXDataset::new(flags, locked_cache.clone(),0)?)))
        }
        else {
            None
        };


        let seed = Arc::new(RwLock::new([0u8;32]));
        let (worker_input, worker_results) = mpsc::channel(WORKER_CHANNEL_SIZE);
        let verifier = Worker::new(None, Some(cache.clone()), seed.clone(), worker_input.clone())?;
        let mut solvers = Vec::new();

        for _ in 0..nb_threads - 1 {
            solvers.push(Worker::new(dataset.clone(),Some(cache.clone()),seed.clone(), worker_input.clone())?);
        }



        let (feed_route, receiver) = mpsc::channel(SOLVER_CHANNEL_SIZE);
        Ok(Solver {
            mode,
            solvers,
            verifier,
            feed_route,
            receiver,
            worker_results,
            solver_output,
        })

    }
}
