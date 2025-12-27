use super::stratum_models::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use crate::DaturaPow;
use super::errors::ClientError;

pub struct Client {
    addr: Option<String>,
    stream: Option<BufReader<TcpStream>>,
    last_job: Option<DaturaPow>,
}

impl Client {
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
                    todo!("write reconnect logic");
                    return ClientError::ServerDisconnected;
                    self.last_job.clone()
                }
                Ok(Ok(_)) => {
                    if let Ok(ServerReply::WorkOrder { params, .. }) =
                        serde_json::from_str::<ServerReply>(&line)
                    {
                        self.last_job = Some(params.clone().into());
                        Ok(params.into())
                    } else {
                        ClientError::UnknownServerReply(line.clone())
                    }
                }
                Ok(Err(e)) => {
                    ClientError::ReadError(e)
                }
                Err(_) => {
                    println!("No new job received in 5 seconds");
                    self.last_job.clone()
                }
            }
        } else {
            Ok(DaturaPow::random())
        }
    }
    pub async fn new(addr: Option<String>) -> Result<Self, ()> {
        if let Some(ip_addr) = addr {
            let mut reader = BufReader::new(
                TcpStream::connect(ip_addr.clone())
                    .await
                    .expect("Connection failed"),
            );
            let login_str = serde_json::to_string(&StratumQuery {
                id: 1,
                jsonrpc: "2.0".to_string(),
                method: "login".to_string(),
                params: StratumParams::LoginParams::default(),
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
