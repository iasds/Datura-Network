use super::errors::SolverError;
use crate::client::JobData;
use crate::consts::*;
use rand::fill;
use std::time::Instant;

///Instant is the deadline
#[derive(Debug, Clone)]
pub enum SolverJob {
    Verify((DaturaPow, Vec<u8>, Instant)),
    Solve((DaturaPow, Instant)),
}

impl SolverJob {
    pub fn print(&self) {
        match self {
            SolverJob::Verify((pow, _, _)) | SolverJob::Solve((pow, _)) => {
                pow.print();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum SolverResult {
    Valid((DaturaPow, Vec<u8>)),
    Invalid((DaturaPow, SolverError)),
    Solved((DaturaPow, Vec<u8>)),
    Error(SolverError),
}

impl SolverResult {
    pub fn print(&self) {
        match self {
            SolverResult::Valid((pow, _))
            | SolverResult::Invalid((pow, _))
            | SolverResult::Solved((pow, _)) => {
                pow.print();
            }
            SolverResult::Error(err) => {
                println!("error: {:?}", err);
            }
        }
    }
}

///Structure for datura network proof of work, can be autonomously generated
///or created from a p2pool job
#[derive(Debug, Clone)]
pub struct DaturaPow {
    pub blob: [u8; 76],
    pub seed_hash: [u8; 32],
    pub job_id: String,
    pub target: String,
}

impl TryFrom<JobData> for DaturaPow {
    type Error = SolverError;
    fn try_from(work_order: JobData) -> Result<Self, SolverError> {
        Ok(DaturaPow {
            job_id: work_order.job_id,
            blob: hex::decode(work_order.blob)?.as_slice().try_into()?,
            seed_hash: hex::decode(work_order.seed_hash)?.as_slice().try_into()?,
            target: work_order.target,
        })
    }
}

impl DaturaPow {
    pub fn print(&self) {
        println!(
            "job_id {}, blob {}, seed_hash {}",
            self.job_id,
            hex::encode(self.blob),
            hex::encode(self.seed_hash)
        );
    }
    pub fn new_nonce(&mut self) {
        fill(&mut self.blob[NONCE_OFFSET..NONCE_OFFSET + NONCE_SIZE]);
    }

    pub fn get_nonce(&self) -> String {
        hex::encode(&self.blob[NONCE_OFFSET..NONCE_OFFSET + NONCE_SIZE])
    }

    ///Create new Datura pow from p2pool job data
    pub fn new(blob: [u8; 76], seed_hash: [u8; 32], job_id: String, target: String) -> Self {
        DaturaPow {
            blob,
            seed_hash,
            job_id,
            target,
        }
    }

    ///generate really random challenge
    pub fn random(seed_hash: [u8; 32]) -> Self {
        let mut blob = [0u8; 76];
        let mut job_id = [0u8; 32];

        fill(&mut blob);
        fill(&mut job_id);

        DaturaPow {
            job_id: hex::encode(job_id),
            blob,
            seed_hash,
            target: MINIMAL_DIFFICULTY.to_string(),
        }
    }
}
