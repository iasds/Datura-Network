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
    pub solver_input: mpsc::Sender<SolverJob>,

    ///Used by the solver tasks to receive new solverjobs
    receiver: mpsc::Receiver<SolverJob>,

    ///Used by the solver to send back results to the outside
    pub solver_output: mpsc::Sender<SolverResult>,
    
    ///used by the solver when a share can be sent back to the pool
    upstream_pool: mpsc::Sender<SolverResult>,

    workers: ThreadPool,
}

#[derive(Copy,Debug,Clone,PartialEq)]
pub enum SolverMode {
    Light,
    Fast,
}

impl Solver {
    ///If creating with multiple threads one will be a dedicated verification thread and priorize
    ///verification work
    pub fn new(mode: SolverMode, mut nb_threads: usize, solver_output: mpsc::Sender<SolverResult>, upstream_pool: mpsc::Sender<SolverResult>) -> Result<Self,SolverError> {
        nb_threads = min(nb_threads,num_cpus::get());
        let flags = get_flags(mode);
        let cache = RandomXCache::new(flags,&[0u8;32])?; //always at least one worker thread
        let dataset = if mode==SolverMode::Fast {
            let locked_cache = cache.read().unwrap();
            RandomXDataset::new(flags, cache.clone(),0)?
        }
        else {
            None
        };

        //create rayon threadpool here, create 1 vm of the right type depending on the mode

        let (solver_input, receiver) = mpsc::channel(SOLVER_CHANNEL_SIZE);
        Ok(Solver {
            mode,
            solver_input
            receiver,
            solver_output,
            upstream_pool,
            pool: ThreadPool::new
        })
    }
}
