use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub trait JobGenerator {
    fn get_job(difficulty: u64, job_type: JobType) -> ([u8; 64], JobType);
    fn submit_result(response: &[u8]) -> Result<(), ()>;
}
pub enum JobType {
    XMR,
    Random,
}

pub struct Client {
    addr: Option<String>,
    stream: Option<BufReader<TcpStream>>,
    last_job: Option<JobData>,
}

#[derive(Default, Serialize, Deserialize)]
struct LoginParams {
    login: String,
    pass: String,
    agent: String,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum StratumQueries {
    Login {
        id: i64,
        jsonrpc: String,
        method: String,
        params: LoginParams,
    }
}

#[derive(Deserialize)]
struct Job {
    id: String,
    job: JobData,
    extensions: Vec<String>,
    status: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct JobData {
    blob: String,
    job_id: String,
    target: String,
    algo: String,
    height: i64,
    seed_hash: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ServerReply {
    LoginReply{
        jsonrpc: String,
        id:i64,
        error: Option<String>,
        result: Job,
    },
    WorkOrder {
        jsonrpc: String,
        method: String,
        params: JobData,
    }
}


impl Client {
    pub async fn get_challenge(&mut self) -> Option<JobData> {
        let mut line = String::new();
        if let Some(reader) = &mut self.stream {
            match tokio::time::timeout(std::time::Duration::from_secs(5), reader.read_line(&mut line)).await {
                Ok(Ok(0)) => {
                    println!("Server closed connection");
                    self.last_job.clone()
                }
                Ok(Ok(_)) => {
                    if let Ok(ServerReply::WorkOrder { params, .. }) = serde_json::from_str::<ServerReply>(&line) {
                        self.last_job = Some(params.clone());
                        self.last_job.clone()
                    }
                    else {
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
        let mut client = Client {
            addr,
            stream: None,
            last_job: None,
        };
        if let Some(ref ip_addr) = client.addr {
            let mut reader = BufReader::new(
                TcpStream::connect(ip_addr)
                    .await
                    .expect("Connection failed"),
            );
            let login_str = serde_json::to_string(&StratumQueries::Login{id: 1, jsonrpc: "2.0".to_string(), method: "login".to_string(), params: LoginParams::default() }).unwrap();
            reader.get_mut().write_all(format!("{}\n",login_str).as_bytes()).await.unwrap();
            let mut line = String::new();
            let res = reader.read_line(&mut line).await.unwrap();
            if let ServerReply::LoginReply{result,..} = serde_json::from_str(&line).unwrap() {
                client.last_job = Some(result.job);
            }
            else {
                panic!("no job data in reply!");
            }
            client.stream = Some(reader);
        }
       
        Ok(client)
    }
}

impl JobGenerator for Client {
    fn get_job(_difficulty: u64, jtype: JobType) -> ([u8; 64], JobType) {
        ([0u8; 64], jtype)
    }

    fn submit_result(_response: &[u8]) -> Result<(), ()> {
        Ok(())
    }
}
