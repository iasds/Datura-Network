use tokio::task;
use randomx_rs::*;
use crate::SolverError;
use crate::solver::worker::{WorkerState,Worker};
use tokio::sync::mpsc;
use crate::consts::*;
use std::sync::{Arc,atomic::{Ordering,AtomicBool}};
use tokio::sync::RwLock;
use super::utils::*;
use std::cmp::min;
use super::models::*;
use tokio::time::{Instant,sleep};
use std::cell::UnsafeCell;

#[derive(Debug)]
pub struct SharedDataset {
    pub dataset: RwLock<UnsafeCell<RandomXDataset>>
}

impl SharedDataset {
    pub fn new(ds: RandomXDataset) -> Self {
        SharedDataset {
            dataset: RwLock::new(UnsafeCell::new(ds)),
        }
    }
    pub fn get(&self) -> RandomXDataset {
        unsafe {
            let guard = self.dataset.blocking_read();
            (*(guard.get())).clone()
        }
    }
}

impl Clone for SharedDataset {
    fn clone(&self) -> Self {
        SharedDataset {
            dataset: RwLock::new(UnsafeCell::new(self.get())),
        }
    }
}

unsafe impl Send for SharedDataset {}
unsafe impl Sync for SharedDataset {}

#[derive(Debug)]
pub struct SharedCache {
    pub cache: RwLock<UnsafeCell<RandomXCache>>,
}

impl Clone for SharedCache {
    fn clone(&self) -> Self {
        SharedCache {
            cache: RwLock::new(UnsafeCell::new( self.get() )),
        }
    }

}

unsafe impl Send for SharedCache {}
unsafe impl Sync for SharedCache {}

impl SharedCache {
    pub fn new(cache: RandomXCache) -> Self {
        SharedCache {
            cache: RwLock::new(UnsafeCell::new(cache)),
        }
    }
    pub fn get(&self) -> RandomXCache {
        unsafe {
            let guard = self.cache.blocking_read();
            (*(guard.get())).clone()
        }
    }

}

pub struct WorkerChannels {
    pub available: Arc<AtomicBool>,
    pub job_channel: mpsc::Sender<SolverJob>,
}



///Solver for challenge completion and verification
///threadnumber serves to force a limit on workers, set to 0 for automatically
///use as many cores as available
pub struct Solver {

    ///define the solver mode: in light mode we only use one or more light workers, in
    ///fast mode we use one light worker for verifications (or hashing if it has nothing else to
    ///do) and the others for everything else
    mode: SolverMode,


    ///use by main solver to receive job
    solver_input: RwLock<mpsc::Receiver<SolverJob>>,


    ///used by workers to send back results
    worker_output_receiver: RwLock<mpsc::Receiver<SolverResult>>,
    worker_output_sender: mpsc::Sender<SolverResult>,


    ///Used by the solver to send back results to the outside
    pub solver_output: mpsc::Sender<SolverResult>,
    
    ///used by the solver when a share can be sent back to the pool
    upstream_pool: mpsc::Sender<SolverResult>,

    seed: Arc<RwLock<[u8;32]>>,
    nb_threads: usize,
    work_allocation: RwLock<Vec<Arc<WorkerChannels>>>,
}


#[derive(Copy,Debug,Clone,PartialEq)]
pub enum SolverMode {
    Light,
    Fast,
}

impl Solver {
    ///If creating with multiple threads one will be a dedicated verification thread and priorize
    ///verification work
    pub fn new(mode: SolverMode, mut nb_threads: usize, solver_input: mpsc::Receiver<SolverJob>, solver_output: mpsc::Sender<SolverResult>, upstream_pool: mpsc::Sender<SolverResult>) -> Result<Arc<Self>,SolverError> {
        nb_threads = min(nb_threads,num_cpus::get());
//always at least one worker thread

        let (worker_output_sender, worker_output_receiver) = mpsc::channel(WORKER_CHANNEL_SIZE);


        Ok(Arc::new(Solver {
            mode,
            nb_threads,
            solver_input: RwLock::new(solver_input),
            worker_output_sender,
            worker_output_receiver: RwLock::new(worker_output_receiver),
            solver_output,
            upstream_pool,
            seed: Arc::new(RwLock::new([0u8;32])),
            work_allocation: RwLock::new(Vec::new()),
        }))
    }

    pub async fn do_work(this: Arc<Self>){
        let seed = this.seed.clone();
        let mode = this.mode;
        let initial_state: WorkerState = task::spawn_blocking(move ||  {
            let seed_guard = seed.blocking_read();
            let cache = Arc::new(
                    SharedCache::new(RandomXCache::new(get_flags(mode), &*seed_guard).unwrap()) 
                );
            match mode {
                SolverMode::Light => {
                    WorkerState::Light {
                        cache
                    }
                }
                SolverMode::Fast => {
                    let _cache_guard = cache.cache.blocking_read();
                    WorkerState::Fast {
                        dataset : Arc::new(
                                          SharedDataset::new(RandomXDataset::new(get_flags(mode), cache.get(),0).unwrap())),
                    }
                }
            }
        }).await.unwrap();

        let mut w_chans = this.work_allocation.write().await;
        for _ in 0..this.nb_threads {
            let (job_sender, job_receiver) = mpsc::channel(1);
            let available = Arc::new(AtomicBool::new(true));
            let mut worker = Worker::new(get_flags(this.mode), initial_state.clone(), this.seed.clone(), job_receiver, this.worker_output_sender.clone(), available.clone()).unwrap();
            let channels = Arc::new(WorkerChannels { available, job_channel: job_sender });
            task::spawn_blocking(move || worker.start());
            w_chans.push(channels);
        }
        let mut solver_input = this.solver_input.write().await;
               
        while let Some(solverjob) = solver_input.recv().await {
            println!("solver: got job: {:?}",solverjob);
            match solverjob {
                SolverJob::Verify((_,_,deadline)) => {
                    //verification job, only one thread required
                    //always give work to the worker who has been working the longest (round robin)
                    while deadline > (Instant::now() + VERIFY_USUAL_DURATION).into() {
                        w_chans.iter().filter(|w|w.available.load(Ordering::Acquire));
                        if let Some(work_allocation) = w_chans.first() {
                            println!("sending verification");
                            work_allocation.job_channel.send(solverjob.clone()).await;
                            break;
                        }
                        else {
                            //wait for the longest possible time a verify job can take and try
                            //again
                            sleep(VERIFY_USUAL_DURATION).await;
                        }
                    }
                }
                SolverJob::Solve((_,deadline)) => {
                    while deadline > (Instant::now() + VERIFY_USUAL_DURATION).into() {
                        if w_chans.iter().filter(|w|w.available.load(Ordering::Acquire)).count() != 0 {
                            //wait for the longest possible time a verify job can take and try
                            //again

                            for w in w_chans.iter().filter(|w|w.available.load(Ordering::Acquire)) {
                                println!("sending solve job");
                                w.job_channel.send(solverjob.clone()).await;
                            }
                            break;
                        }
                        else {
                            sleep(VERIFY_USUAL_DURATION).await;
                        }
                    }
                }
            }
        }
    }
}
