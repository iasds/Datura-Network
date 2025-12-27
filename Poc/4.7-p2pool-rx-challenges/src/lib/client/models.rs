use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StratumParams {
    LoginParams {
    login: String,
    pass: String,
    agent: String,
},
    SubmitParams {
        id: String,
        job_id: String,
        nonce: String,
        result: String,
    }
}

#[derive(serde::Serialize, Deserialize)]
pub struct StratumQuery {
        id: i64,
        jsonrpc: String,
        method: String,
        params: StratumParams,
    }


#[derive(Deserialize)]
pub struct Job {
    pub id: String,
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
