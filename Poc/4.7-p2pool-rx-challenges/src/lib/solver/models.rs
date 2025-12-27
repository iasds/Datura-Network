use rand::fill;
use crate::client::JobData;
use super::errors::SolverError;

///Structure for datura network proof of work, can be autonomously generated
///or created from a p2pool job
#[derive(Debug)]
pub struct DaturaPow {
    blob: Vec<u8>,
    seed_hash: [u8;32],
    job_id: String,
    target: u64,
}

impl Iterator for DaturaPow {
    type Item = DaturaPow;

    ///Iterate over the possible DaturaPows, we are emitting jobs as if we were a pool
    ///so we are using the first 16 bit of our 32 bit nonce per client and send a random lower 16
    ///bits for the client to iterate over
    fn next(&mut self) -> Option<Self::Item> {
        let mut newblob = self.blob.clone();

           // Example: upper 16 bits already set for client
        let upper = ((newblob[124] as u16) | ((newblob[125] as u16) << 8)) as u16;

        // Iterate lower 16 bits
        let full_nonce = ((upper as u32) << 16) | 0u32;

        newblob[124..128].copy_from_slice(&full_nonce.to_le_bytes());
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
    fn try_from(work_order: JobData) -> Result<DaturaPow,SolverError> {
        println!("{:?}",work_order);
        let tblob =hex::decode(work_order.blob.clone()).unwrap();
        println!("iblob len {}",tblob.len());
        Ok(DaturaPow {
            job_id: work_order.job_id,
            blob: hex::decode(work_order.blob)?,
            seed_hash: hex::decode(work_order.seed_hash)?.as_slice().try_into()?,
            target: u64::from_str_radix(&work_order.target,16)?,
        })
    }
}

impl DaturaPow {
    ///Create new Datura pow from p2pool job data
    pub fn new(blob: Vec<u8>, seed_hash: [u8; 32], job_id: String, target: u64) -> Self {
        DaturaPow {
            blob,
            seed_hash,
            job_id,
            target,
        }
    }
    
        ///generate really random challenge
    pub fn random(target: Option<u64>) -> Self {
        
        let mut blob = [0u8;128];
        let mut seed_hash = [0u8;32];

        fill(&mut blob);
        fill(&mut seed_hash);
        DaturaPow {
            job_id: "0".to_string(),
            blob: blob.to_vec(),
            seed_hash,
            target: target.unwrap_or(1),
        }
    }
}


