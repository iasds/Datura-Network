use super::models::*;
use std::ops::Add;
use std::sync::Arc;
use rand::fill;
use std::collections::HashMap;
use tokio::io::{BufReader,AsyncBufReadExt,AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::spawn;
use crate::solver::*;
use super::errors::*;
use std::time::Instant;
use tokio::time::{Duration,sleep};
use tokio::sync::{mpsc,RwLock};
use crate::consts;

#[derive(Debug)]
struct ShareInfo {
    pub target: u64,
    pub date: Instant,
}

pub struct Client {
    random_seed : RwLock<([u8;32],Instant)>,
    stream: Option<RwLock<BufReader<TcpStream>>>,
    last_datura_pow: RwLock<DaturaPow>,
    last_id: RwLock<i64>,
    worker_id: RwLock<String>,
    job_list: RwLock<HashMap<String,ShareInfo>>,
    submission_channel: RwLock<mpsc::Receiver<SolverResult>>,
}

pub type P2poolReply = Result<(),PoolError>;

impl Client {


    pub async fn start(this: Arc<Self>) {
            let me = this.clone();
        if this.stream.is_some() {
            spawn(Self::retrieve_challenges(me));
        }
        else {
            spawn(Self::drop_challenges(me));
        }
            let me = this.clone();
        spawn( Self::maintenance_task(me));
    }

    pub async fn maintenance_task(this: Arc<Self>) {
        loop {
            //remove all expired jobs

            let mut job_list_guard = this.job_list.write().await;
            job_list_guard.retain(|_,ShareInfo { date,..}| *date > Instant::now());
            drop(job_list_guard);
            let mut r_seed_guard = this.random_seed.write().await;
            if r_seed_guard.1 < Instant::now() {
                //time for a new random_seed
                let mut new_seed = [0u8;32];
                fill(&mut new_seed);
                *r_seed_guard = (new_seed, Instant::now().add(consts::SEED_LIFETIME));
            }
            drop(r_seed_guard);
            sleep(consts::POW_MAX_LIFETIME).await;
        }
    }

    pub async fn get_solver_job(this: Arc<Self>) -> SolverJob {
        let mut job_list = this.job_list.write().await;
        let mut last_datura_pow = this.last_datura_pow.write().await;
        let random_seed = this.random_seed.read().await;

        let shareinfo = job_list.get(&last_datura_pow.job_id).unwrap();

        if shareinfo.date > Instant::now() {
            println!("reusing last pow");
            let last_datura_pow = this.last_datura_pow.read().await;
           SolverJob::Solve((last_datura_pow.clone(),shareinfo.date))
        }
        else {
            println!("creating new pow");
            let pow = DaturaPow::random(None, random_seed.0.clone());
            let expiration_date = Instant::now().add(consts::POW_MAX_LIFETIME);
            job_list.insert(pow.job_id.clone(), ShareInfo{target: pow.target, date: expiration_date});
            *last_datura_pow = pow.clone();
            SolverJob::Solve((pow, expiration_date))
        }
    }

    pub async fn retrieve_challenges(this: Arc<Self>) {
            let mut line = String::new();
            if let Some( reader) = &this.stream {
                let mut read_guard = reader.write().await;
            let mut submission_channel = this.submission_channel.write().await;
            loop {
                tokio::select! {
                    _ = read_guard.read_line(&mut line) => {
                            println!("got new challenge from server: {}",line);
                            if let Ok(ServerReply::WorkOrder { params, .. }) =
                                serde_json::from_str::<ServerReply>(&line)
                            {
                                let pow: DaturaPow = params.clone().try_into().unwrap();
                                let mut job_list = this.job_list.write().await;
                                job_list.insert(params.job_id.clone(), ShareInfo { target: pow.target, date: Instant::now() });

                                let mut last_datura_pow = this.last_datura_pow.write().await;
                                *last_datura_pow = pow;
                            } 
                    }
                    Some(solver_output) = submission_channel.recv() => {
                                println!("got new output submission: {:?}",solver_output);
                                if let SolverResult::Valid((pow,solution))  = solver_output {
                                    
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
                    }

                    }
                }
            }
            }

    pub async fn drop_challenges(this: Arc<Self>) {
        let mut submission_channel = this.submission_channel.write().await;
        while let Some(_) = submission_channel.recv().await {
            println!("running in local mode, dropping solution");
            sleep(Duration::from_millis(500));
        }
    }

    pub async fn new(addr: Option<String>, submission_channel: mpsc::Receiver<SolverResult>) -> Result<Arc<Self>, ClientError> {
        let mut job_list = HashMap::new();
        let mut r_seed = [0u8;32];
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
            let (last_job,worker_id): (JobData,String) = if let ServerReply::LoginReply { result, .. } =
                serde_json::from_str(&line).unwrap()
            {
                (result.job,result.id.clone())
            } else {
                panic!("login failed");
            };
            let last_datura_pow:DaturaPow =  {
                let pow: DaturaPow = last_job.clone().try_into()?;
                job_list.insert(last_job.job_id.clone(),ShareInfo{ date: Instant::now(),target: pow.target});
                pow
            };
            return Ok(Arc::new(Client {
                random_seed: RwLock::new((r_seed, Instant::now().add(consts::SEED_LIFETIME))),
                stream: Some(RwLock::new(reader)),
                last_datura_pow: RwLock::new(last_datura_pow),
                last_id : RwLock::new(2),
                worker_id: RwLock::new(worker_id),
                job_list: RwLock::new(job_list),
                submission_channel: RwLock::new(submission_channel),
            }));
        }
        let pow = DaturaPow::random(None,r_seed.clone());
        job_list.insert(pow.job_id.clone(),ShareInfo { date: Instant::now(), target: pow.target});
        Ok(Arc::new(Client {
            last_datura_pow: RwLock::new(pow),
            random_seed: RwLock::new((r_seed, Instant::now().add(consts::SEED_LIFETIME))),
            stream: None,
            last_id : RwLock::new(1),
            worker_id: RwLock::new(String::new()),
            job_list: RwLock::new(job_list),
            submission_channel: RwLock::new(submission_channel),
        }))
    }
}
