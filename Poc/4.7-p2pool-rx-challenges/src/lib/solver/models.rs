use rand::fill;
use crate::client::JobData;
use super::errors::SolverError;

///Structure for datura network proof of work, can be autonomously generated
///or created from a p2pool job
pub struct DaturaPow {
    blob: [u8;128],
    seed_hash: [u8;32],
    job_id: String,
}

impl Iterator for DaturaPow {
    type Item = DaturaPow;

    ///Iterate over the possible DaturaPows, we are emitting jobs as if we were a pool
    ///so we are using the first 16 bit of our 32 bit nonce per client and send a random lower 16
    ///bits for the client to iterate over
    fn next(&mut self) -> Option<Self::Item> {
        let mut newblob = self.blob;

           // Example: upper 16 bits already set for client
        let upper = ((newblob[124] as u16) | ((newblob[125] as u16) << 8)) as u16;

        // Iterate lower 16 bits
        let full_nonce = ((upper as u32) << 16) | 0u32;

        newblob[124..128].copy_from_slice(&full_nonce.to_le_bytes());
        self.blob = newblob.clone();
        
        
        Some(DaturaPow {
            seed_hash: self.seed_hash.clone(),
            blob: newblob,
            job_id: self.job_id.clone()
        })
    }
}

impl TryFrom<JobData> for DaturaPow {
    type Error = SolverError;
    fn try_from(workOrder: JobData) -> Result<DaturaPow,SolverError> {
        Ok(DaturaPow {
            job_id: workOrder.job_id,
            blob: hex::decode(workOrder.blob)?.as_slice().try_into()?,
            seed_hash: hex::decode(workOrder.seed_hash)?.as_slice().try_into()?,

        })
    }
}

impl DaturaPow {
    ///Create new Datura pow from p2pool job data
    pub fn new(blob: [u8;128], seed_hash: [u8; 32], job_id: String) -> Self {
        DaturaPow {
            blob,
            seed_hash,
            job_id,
        }
    }
    
        ///generate really random challenge
    pub fn random() -> Self {
        
        let mut blob = [0u8;128];
        let mut seed_hash = [0u8;32];

        fill(&mut blob);
        fill(&mut seed_hash);
        DaturaPow {
            job_id: "0".to_string(),
            blob,
            seed_hash,
        }
    }
}


