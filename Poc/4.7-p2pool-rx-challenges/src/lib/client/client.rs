use super::models::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use crate::solver::DaturaPow;
use super::errors::ClientError;

pub struct Client {
    addr: Option<String>,
    stream: Option<BufReader<TcpStream>>,
    last_job: Option<JobData>,
    last_datura_pow: DaturaPow,
    last_id: i64,
}

impl Client {
    pub async fn submit_solution(&mut self, pow: DaturaPow, solution: Vec<u8>) -> Result<(),ClientError> {
        self.last_id += 1;
        let submission = StratumQuery::new(self.last_id, StratumParams::SubmitParams {
            id: self.last_id,
            job_id: pow.job_id,
            nonce: pow.nonce.to_string(),

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
            let last_job = if let ServerReply::LoginReply { result, .. } =
                serde_json::from_str(&line).unwrap()
            {
                Some(result.job)
            } else {
                None
            };
            let last_datura_pow = if let Some(job) = &last_job {
                println!("creating datura pow from {:?}",job);
                job.clone().try_into()?
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
            });
        }
        Ok(Client {
            addr: None,
            stream: None,
            last_job: None,
            last_datura_pow : DaturaPow::random(None,None), //implement backpressure with higher diff
            last_id : 1,
        })
    }
}
