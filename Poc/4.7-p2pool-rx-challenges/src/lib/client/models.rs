/// This file contains models for interacting with the p2pool server
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
///Type of params used when communicating with the server as queries are always the same
///but for the param field
pub enum StratumParams {
    ///Login variant: all those strings can be empty
    LoginParams {
    login: String,
    pass: String,
    agent: String,
},
    ///job submission params for converting back daturapows to p2pool shares
    SubmitParams {
        id: String,
        job_id: String,
        nonce: String,
        result: String,
    }
}

impl StratumParams {
    pub fn empty_login() -> Self {
        Self::LoginParams {
            login: "DaturaNet Worker".to_string(),
            pass: String::new(),
            agent: String::new(),
        }
    }
}

#[derive(serde::Serialize, Deserialize)]
pub struct StratumQuery {
        id: i64,
        jsonrpc: String,
        method: String,
        params: StratumParams,
    }

impl StratumQuery {
    pub fn new(id: i64, method: String, params: StratumParams) -> Self {
        StratumQuery{
            id,
            method,
            jsonrpc: "2.0".to_string(),
            params,
        }
    }
}


#[derive(Deserialize, Debug)]
pub struct Job {
    pub id: String,
    pub job: JobData,
    extensions: Vec<String>,
    status: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct JobData {
    pub blob: String,
    pub job_id: String,
    pub target: String,
    algo: String,
    height: u64,
    pub seed_hash: String,
}

#[derive(Deserialize, Debug)]
pub struct MinerLoginReply{
    pub id: String,
    pub job: JobData,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum ServerReply {
    LoginReply {
        jsonrpc: String,
        id: i64,
        error: Option<String>,
        result: MinerLoginReply,
    },
    WorkOrder {
        jsonrpc: String,
        method: String,
        params: JobData,
    },
    Unknown,
}
