use super::models::*;
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use crate::solver::DaturaPow;
use super::errors::ClientError;
use std::time::Instant;
use super::solver::check_hash;

struct ShareInfo {
    pub target: u64,
    pub date: Instant,
}

pub struct Client {
    addr: Option<String>,
    stream: Option<BufReader<TcpStream>>,
    last_job: Option<JobData>,
    last_datura_pow: DaturaPow,
    last_id: i64,
    worker_id: Option<String>,
    job_list: HashMap<String,ShareInfo>,
}

impl Client {
    pub async fn submit_solution(&mut self, pow: DaturaPow, solution: Vec<u8>) -> Result<(),ClientError> {
        if let Some(ShareInfo{target,..}) = &self.job_list.get(&pow.job_id) {
            if check_hash(solution.as_slice(), target) {
        self.last_id += 1;
        let submission = StratumQuery::new(self.last_id,"submit".to_string(), StratumParams::SubmitParams {
            id: self.worker_id.clone().unwrap(),
            job_id: pow.job_id,
            nonce: pow.nonce.to_string(),
            result: hex::encode(&solution),
        });
            let submission_str = serde_json::to_string(&submission)?;

            if let Some(reader) = &mut self.stream {
                reader
                    .get_mut()
                    .write_all(format!("{}\n", submission_str).as_bytes())
                    .await?;
            }


            }
            else {
                println!("result target {} is lower than recorded {}, not submitting",pow.target,target);
            }
        }
        else {
            println!("not in the list");
        }
            Ok(())


    }

    //reimplement as stream with a running loop
    pub async fn get_challenge(&mut self) -> Result<DaturaPow,ClientError> {
        let mut line = String::new();
        if let Some(reader) = &mut self.stream {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                reader.read_line(&mut line),
            )
            .await
            {
                Ok(Ok(0)) => {
                    Err(ClientError::ServerDisconnected)
                }
                Ok(Ok(_)) => {
                    if let Ok(ServerReply::WorkOrder { params, .. }) =
                        serde_json::from_str::<ServerReply>(&line)
                    {
                        self.last_job = Some(params.clone().into());
                        Ok(params.try_into().unwrap())
                    } else {
                        Err(ClientError::UnknownServerReply(line.clone()))
                    }
                }
                Ok(Err(e)) => {
                    Err(ClientError::ReadError(e.to_string()))
                }
                Err(_) => {
                    println!("No new job received in 5 seconds");
                    Ok(self.last_datura_pow.next().unwrap()) //can't fail
                }
            }
        } else {
            Ok(DaturaPow::random(None,None))
        }
    }
    pub async fn new(addr: Option<String>) -> Result<Self, ClientError> {
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
            let (last_job,worker_id): (Option<JobData>,Option<String>) = if let ServerReply::LoginReply { result, .. } =
                serde_json::from_str(&line).unwrap()
            {
                (Some(result.job),Some(result.id.clone()))
            } else {
                (None,None)
            };
            let last_datura_pow:DaturaPow = if let Some(job) = &last_job {
                let pow = job.clone().try_into()?;
                self.job_list.insert(job.job_id,ShareInfo{ date: Instant::now(),target: pow.target});
                pow
            }
            else {
                DaturaPow::random(None,None)
            };
            return Ok(Client {
                addr: Some(ip_addr.clone()),
                stream: Some(reader),
                last_job,
                last_datura_pow,
                last_id : 2,
                worker_id,
                job_list: HashMap::new(),
            });
        }
        Ok(Client {
            addr: None,
            stream: None,
            last_job: None,
            last_datura_pow : DaturaPow::random(None,None), //implement backpressure with higher diff
            last_id : 1,
            worker_id: None,
            job_list: HashMap::new(),
        })
    }
}
