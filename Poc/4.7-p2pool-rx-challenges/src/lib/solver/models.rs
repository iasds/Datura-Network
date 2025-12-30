use crate::consts::*;
use rand::fill;
use crate::client::JobData;
use super::errors::SolverError;
use std::time::Duration;

#[derive(Debug)]
pub enum SolverJob {
    Verify((DaturaPow,Vec<u8>)),
    Solve((DaturaPow, Duration)),
}

impl SolverJob {
    pub fn get_pow(&self) -> &DaturaPow {
        match self {
            Self::Verify((pow,_))|Self::Solve((pow,_)) => &pow,
        }
    }
}

#[derive(Debug)]
pub enum SolverResult {
    Valid((DaturaPow,Vec<u8>)),
    Invalid((DaturaPow,SolverError)),
    Solved((DaturaPow,Vec<u8>)),
    Error(SolverError),
}

///Structure for datura network proof of work, can be autonomously generated
///or created from a p2pool job
#[derive(Debug,Clone)]
pub struct DaturaPow {
    pub blob: [u8;76],
    pub seed_hash: [u8;32],
    pub job_id: String,
    pub target: u64,
}

impl Iterator for DaturaPow {
    type Item = DaturaPow;

    ///Iterate over the possible DaturaPows, we are emitting jobs as if we were a pool
    ///so we are using the first 16 bit of our 32 bit nonce per client and send a 0 lower 16
    ///bits for the client to iterate over
    fn next(&mut self) -> Option<Self::Item> {
        let mut newblob = self.blob;

        let client =  u16::wrapping_add(u16::from_le_bytes(newblob[72..74].try_into().unwrap()), 1);

        // Iterate lower 16 bits

        newblob[72..74].copy_from_slice(&client.to_le_bytes());
        newblob[74..].copy_from_slice(&[0u8;2]);
        self.blob = newblob.clone();
        
        
        Some(DaturaPow {
            seed_hash: self.seed_hash.clone(),
            blob: newblob,
            job_id: self.job_id.clone(),
            target: self.target,
        })
    }
}

impl TryFrom<JobData> for DaturaPow {
    type Error = SolverError;
    fn try_from(work_order: JobData) -> Result<Self,SolverError> {
        println!("creating daturapow from blob {} and seedhash {}",&work_order.blob, &work_order.seed_hash);
        Ok(DaturaPow {
            job_id: work_order.job_id,
            blob: hex::decode(work_order.blob)?.as_slice().try_into()?,
            seed_hash: hex::decode(work_order.seed_hash)?.as_slice().try_into()?,
            target: u64::from_str_radix(&work_order.target,16)?,
        })
    }
}

impl DaturaPow {
    pub fn new_nonce(&mut self) {
        fill(&mut self.blob[NONCE_OFFSET..NONCE_OFFSET + NONCE_SIZE]);
    }

    pub fn get_nonce(&self) -> String {
        hex::encode(&self.blob[NONCE_OFFSET..NONCE_OFFSET + NONCE_SIZE])
    }

    ///Create new Datura pow from p2pool job data
    pub fn new(blob: [u8;76], seed_hash: [u8; 32], job_id: String, target: u64) -> Self {
        DaturaPow {
            blob,
            seed_hash,
            job_id,
            target,
        }
    }

        ///generate really random challenge
    pub fn random(target: Option<u64>, seed: Option<[u8;32]>) -> Self {
        
        let mut blob = [0u8;76];
        let mut seed_hash = seed.unwrap_or([0u8;32]);

        fill(&mut blob);

        if seed.is_none(){
            fill(&mut seed_hash);
        }
        DaturaPow {
            job_id: "0".to_string(),
            blob,
            seed_hash,
            target: target.unwrap_or(1),
        }
    }
}


