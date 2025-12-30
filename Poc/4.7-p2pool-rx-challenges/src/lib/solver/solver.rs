use rayon::ThreadPool;
use randomx_rs::*;
use crate::SolverError;
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

    pool: ThreadPool,
    seed: Arc<RwLock<[u8;32]>>,
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
            solver_input,
            receiver,
            solver_output,
            upstream_pool,
            pool,
            seed: Arc::new(RwLock::new([0u8;32])),
        })
    }

    pub async fn do_work(&mut self){
        let mut vm = None;
        let cache = Arc::new(RwLock::new(None));
        let dataset = Arc::new(RwLock::new(None));
        self.pool.broadcast(||

               
        while let Some(solverjob) = self.receiver.recv().await {
            let pow = solverjob.get_pow();
            let seed_guard = self.seed.read().unwrap();
            if pow.seed_hash !=  *seed_guard {
                drop(seed_guard);
                let seed_guard = self.seed.write().unwrap();
                *seed_guard = pow.seed_hash;
                self.pool.install(|| {
                    let vm = {
            let flags = get_flags(self.mode);
            let cache = RandomXCache::new(flags,&*seed_guard).unwrap(); 
            if self.mode == SolverMode::Light {
            RandomXVM::new(flags, Some(cache.clone()), None).unwrap()
            }
            else {

            let dataset = RandomXDataset::new(flags, cache,0).unwrap();
            RandomXVM::new(flags, None, Some(dataset)).unwrap()
            }
            };
                    tx.blocking_send(vm);
                });

                vm = rx.recv().await;
                //spawn a dedicated thread and await for a new cache/dataset to update the vm
            }
            //install the verify task (only one runner), broadcast the solver jobs
            if let SolverJob::Verify((pow,solution)) = solverjob {
            }
        }
    }
}
