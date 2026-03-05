use crate::consts;
use crate::solver::utils::get_difficulty;
use crate::solver::utils::hash_to_difficulty;
use crate::solver::*;
use rand::fill;
use std::ops::Add;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{RwLock, mpsc};
use tokio::task::spawn;
use tokio::time::{Duration, sleep};

mod errors;
mod models;
pub use errors::*;
pub use models::*;

/// Client for connecting to p2pool/xmrig proxy and retrieve jobs for making
/// datura proof of work
pub struct Client {
    random_seed: RwLock<([u8; 32], Instant)>,
    stream: Option<RwLock<BufReader<TcpStream>>>,
    last_datura_pow: RwLock<DaturaPow>,
    last_id: RwLock<i64>,
    worker_id: RwLock<String>,
    submission_channel: Option<RwLock<mpsc::Receiver<SolverResult>>>,
}

impl Client {
    /// start this client (must be called from the module level
    /// to allow for multithreading
    pub async fn start(this: Arc<Self>) {
        let me = this.clone();
        spawn(Self::retrieve_challenges(me));
        let me = this.clone();
        spawn(Self::maintenance_task(me));
    }

    async fn maintenance_task(this: Arc<Self>) {
        loop {
            let mut r_seed_guard = this.random_seed.write().await;
            if r_seed_guard.1 < Instant::now() {
                //time for a new random_seed
                let mut new_seed = [0u8; 32];
                fill(&mut new_seed);
                *r_seed_guard = (new_seed, Instant::now().add(consts::SEED_LIFETIME));
            }
            drop(r_seed_guard);
            sleep(consts::SEED_LIFETIME.add(Duration::from_secs(1))).await;
        }
    }

    /// asynchronously obtain a SolverJob object, this will always either return a job based
    /// on the latest p2pool job or a random daturapow if running in local-only mode
    pub async fn get_solver_job(this: Arc<Self>) -> SolverJob {
        let last_datura_pow = this.last_datura_pow.read().await;
        SolverJob::Solve((
            last_datura_pow.clone(),
            Instant::now().add(consts::POW_MAX_LIFETIME),
        ))
    }

    async fn retrieve_challenges(this: Arc<Self>) {
        if let (Some(reader), Some(sub_channel)) = (&this.stream, &this.submission_channel) {
            let mut line = String::new();
            let mut read_guard = reader.write().await;
            let mut submission_channel = sub_channel.write().await;
            loop {
                tokio::select! {
                _ = read_guard.read_line(&mut line) => {
                        if let Ok(ServerReply::WorkOrder { params, .. }) =
                            serde_json::from_str::<ServerReply>(&line)
                        {
                            let pow: DaturaPow = params.clone().try_into().unwrap();

                            let mut last_datura_pow = this.last_datura_pow.write().await;
                            *last_datura_pow = pow;
                        }
                        else {
                            println!("received {}",line);
                        }
                        line.clear();
                }
                Some(solver_output) = submission_channel.recv() => {

                                println!("client received a solved job");
                                if let SolverResult::Valid((pow,solution))  = solver_output {
                                    let last = this.last_datura_pow.read().await;
                                    let last_target = get_difficulty(&last.target).unwrap();
                                    drop(last);

                                    let difficulty = hash_to_difficulty(solution.as_slice().try_into().unwrap());
                                    if difficulty >= last_target {
                                        println!("result difficulty: {}, target diff: {}",difficulty, last_target);
                                        let mut last_id_guard = this.last_id.write().await;
                                        *last_id_guard += 1;
                                        let worker_id = this.worker_id.read().await;
                                        let submission = StratumQuery::new(*last_id_guard,"submit".to_string(), StratumParams::SubmitParams {
                                            id: worker_id.clone(),
                                            job_id: pow.job_id.clone(),
                                            nonce: pow.get_nonce(),
                                            result: hex::encode(&solution),
                                        });
                                            let submission_str = serde_json::to_string(&submission).unwrap();

                                                read_guard
                                                    .get_mut()
                                                    .write_all(format!("{}\n", submission_str).as_bytes())
                                                    .await.unwrap();
                                    }
                                    else {
                                        println!("result diff is too low: {} below target {}", difficulty, last_target);
                                    }
                                }
                                else {
                                    println!("bad job type {:?}",solver_output);
                                }

                }

                }
            }
        } else {
            loop {
                let random_seed = this.random_seed.read().await;
                let pow = DaturaPow::random(random_seed.0);
                let mut last_datura_pow = this.last_datura_pow.write().await;
                *last_datura_pow = pow.clone();
                drop(last_datura_pow);
                sleep(consts::POW_MAX_LIFETIME).await;
            }
        }
    }

    ///create a new client with an optional address (set to None if running local only),
    ///the receiver is required for linking with a Solver object
    pub async fn new(
        addr: Option<String>,
        submission_channel: Option<mpsc::Receiver<SolverResult>>,
    ) -> Result<Arc<Self>, ClientError> {
        match (addr.is_some(), submission_channel.is_some()) {
            (true, false) => {
                return Err(ClientError::InitializationError(
                    "address provided but no pool submission channel".to_string(),
                ));
            }
            (false, true) => {
                return Err(ClientError::InitializationError(
                    "pool submission provided but no server address".to_string(),
                ));
            }
            _ => {}
        }

        let mut r_seed = [0u8; 32];
        fill(&mut r_seed);

        if let Some(ip_addr) = addr {
            let mut reader = BufReader::new(
                TcpStream::connect(ip_addr.clone())
                    .await
                    .expect("Connection failed"),
            );
            let login_str = serde_json::to_string(&StratumQuery::new(
                1,
                "login".to_string(),
                StratumParams::empty_login(),
            ))?;
            reader
                .get_mut()
                .write_all(format!("{}\n", login_str).as_bytes())
                .await?;
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let (last_job, worker_id): (JobData, String) = if let ServerReply::LoginReply {
                result,
                ..
            } =
                serde_json::from_str(&line).unwrap()
            {
                (result.job, result.id.clone())
            } else {
                panic!("login failed");
            };
            let last_datura_pow: DaturaPow = last_job.clone().try_into()?;

            return Ok(Arc::new(Client {
                random_seed: RwLock::new((r_seed, Instant::now().add(consts::SEED_LIFETIME))),
                stream: Some(RwLock::new(reader)),
                last_datura_pow: RwLock::new(last_datura_pow),
                last_id: RwLock::new(2),
                worker_id: RwLock::new(worker_id),
                submission_channel: Some(RwLock::new(submission_channel.unwrap())),
            }));
        }
        let pow = DaturaPow::random(r_seed);
        Ok(Arc::new(Client {
            last_datura_pow: RwLock::new(pow),
            random_seed: RwLock::new((r_seed, Instant::now().add(consts::SEED_LIFETIME))),
            stream: None,
            last_id: RwLock::new(1),
            worker_id: RwLock::new(String::new()),
            submission_channel: None,
        }))
    }
}
