use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[derive(Default, Serialize, Deserialize)]
pub struct LoginParams {
    login: String,
    pass: String,
    agent: String,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum StratumQueries {
    Login {
        id: i64,
        jsonrpc: String,
        method: String,
        params: LoginParams,
    },
}

#[derive(Deserialize)]
pub struct Job {
    id: String,
    pub job: JobData,
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
pub enum ServerReply {
    LoginReply {
        jsonrpc: String,
        id: i64,
        error: Option<String>,
        result: Job,
    },
    WorkOrder {
        jsonrpc: String,
        method: String,
        params: JobData,
    },
}
