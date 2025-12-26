use super::stratum_models::*;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub struct Client {
    addr: Option<String>,
    stream: Option<BufReader<TcpStream>>,
    last_job: Option<JobData>,
}

impl Client {
    pub async fn get_challenge(&mut self) -> Option<JobData> {
        let mut line = String::new();
        if let Some(reader) = &mut self.stream {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                reader.read_line(&mut line),
            )
            .await
            {
                Ok(Ok(0)) => {
                    println!("Server closed connection");
                    self.last_job.clone()
                }
                Ok(Ok(_)) => {
                    if let Ok(ServerReply::WorkOrder { params, .. }) =
                        serde_json::from_str::<ServerReply>(&line)
                    {
                        self.last_job = Some(params.clone());
                        self.last_job.clone()
                    } else {
                        panic!("bad reply type");
                    }
                }
                Ok(Err(e)) => {
                    panic!("Read error: {}", e);
                }
                Err(_) => {
                    // timeout
                    println!("No new job received in 5 seconds");
                    self.last_job.clone()
                }
            }
        } else {
            todo!("return some dummy random job");
            None
        }
    }
    pub async fn new(addr: Option<String>) -> Result<Self, ()> {
        if let Some(ip_addr) = addr {
            let mut reader = BufReader::new(
                TcpStream::connect(ip_addr.clone())
                    .await
                    .expect("Connection failed"),
            );
            let login_str = serde_json::to_string(&StratumQueries::Login {
                id: 1,
                jsonrpc: "2.0".to_string(),
                method: "login".to_string(),
                params: LoginParams::default(),
            })
            .unwrap();
            reader
                .get_mut()
                .write_all(format!("{}\n", login_str).as_bytes())
                .await
                .unwrap();
            let mut line = String::new();
            let res = reader.read_line(&mut line).await.unwrap();
            let last_job = if let ServerReply::LoginReply { result, .. } =
                serde_json::from_str(&line).unwrap()
            {
                Some(result.job)
            } else {
                None
            };
            return Ok(Client {
                addr: Some(ip_addr.clone()),
                stream: Some(reader),
                last_job,
            });
        }
        Ok(Client {
            addr: None,
            stream: None,
            last_job: None,
        })
    }
}
