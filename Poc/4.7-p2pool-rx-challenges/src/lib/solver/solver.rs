use rayon::ThreadPool;
use tokio::task;
use randomx_rs::*;
use crate::SolverError;
use tokio::sync::mpsc;
use crate::consts::*;
use std::sync::{Arc,RwLock};
use super::utils::*;
use std::cmp::min;
use super::models::*;
use std::time::Instant;

type WorkPackage = (Option<SolverJob>,time::Instant);

pub struct WorkerAllocation {
    worker: Worker,
    pub job_channel: mpsc::Sender<SolverJob>,
    pub last_job_date: time::Instant,
}



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

    pool: ThreadPool,
    seed: Arc<RwLock<[u8;32]>>,
    nb_threads: usize,
    workers: Vec<WorkerAllocation>,
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
//always at least one worker thread

        let pool = rayon::ThreadPoolBuilder::new().num_threads(nb_threads).build().unwrap();


        let (solver_input, receiver) = mpsc::channel(SOLVER_CHANNEL_SIZE);
        Ok(Solver {
            mode,
            nb_threads,
            solver_input,
            receiver,
            solver_output,
            upstream_pool,
            pool,
            seed: Arc::new(RwLock::new([0u8;32])),
            work_allocation: Vec::new();
        })
    }

    pub async fn do_work(&mut self){
        let initial_state = task::spawn_blocking(|| {
            let seed_guard = self.seed.read().unwrap();
            let cache = RandomXCache::new(self.flags, &*seed_guard).unwrap();
            match self.mode {
                SolverMode::Light => {
                    WorkerState::Light {
                        cache : Arc::new(RwLock::new(cache)),
                        vm: None
                    }
                }
                SolverMode::Fast {
                    WorkerState::Fast {
                        dataset : Arc::new(RwLock::new(RandomXDataset::new(self.flags, cache,0).unwrap())),
                        vm: None,
                    }
                }
            }
        }).await;

        for _ in 0..self.nb_threads {
            let (job_sender, mut job_receiver) = mpsc::channel(1);
            let mut worker = Worker::new(flags, initial_state.clone(), self.mode, self.seed.clone(), job_receiver, result_sender);
            self.pool.install(||worker.start());
            self.work_allocation.push(WorkAllocation { worker, last_job_date: Instant::now(), job_channel: job_sender});
        }
               
        while let Some(solverjob) = self.receiver.recv().await {
            match solverJob {
                SolverJob::Verify(_) => {
                    //verification job, only one thread required
                    //always give work to the worker who has been working the longest (round robin)
                    self.work_allocation.sort_by_key(|w|w.last_job_date).reverse();
                    if let Some(work_allocation) = self.work_allocation.first() {
                        work_allocation.job_channel.send(solverJob.clone()).await;
                        work_allocation.last_job_date = Instant::now();
                    }
                }
                SolverJob::Solve(_) => {

                }

        }
    }
}
