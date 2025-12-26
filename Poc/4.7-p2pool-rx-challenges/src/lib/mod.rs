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
    rq_id: i64,
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


impl Client {
    pub async fn new(addr: Option<String>) -> Result<Self, ()> {
        let mut client = Client {
            addr,
            stream: None,
            rq_id: 1,
        };
        if let Some(ref ip_addr) = client.addr {
            let mut reader = BufReader::new(
                TcpStream::connect(ip_addr)
                    .await
                    .expect("Connection failed"),
            );
            let login_str = serde_json::to_string(&StratumQueries::Login{id: 1, jsonrpc: "2.0".to_string(), method: "login".to_string(), params: LoginParams::default() }).unwrap();
            println!("{}",login_str);
            reader.get_mut().write_all(format!("{}\n",login_str).as_bytes()).await.unwrap();

            let mut line = String::new();
            let res = reader.read_line(&mut line).await.unwrap();
            println!("subscribe response: {}, {} long", line, res);
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
