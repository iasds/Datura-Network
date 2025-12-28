use rand::fill;
use crate::client::JobData;
use super::errors::SolverError;

///Structure for datura network proof of work, can be autonomously generated
///or created from a p2pool job
#[derive(Debug,Clone)]
pub struct DaturaPow {
    pub blob: [u8;76],
    pub seed_hash: [u8;32],
    job_id: String,
    pub target: u64,

    ///if this pow is sent back as a solution then its nonce will be set
    pub nonce: u16,
}

impl Iterator for DaturaPow {
    type Item = DaturaPow;

    ///Iterate over the possible DaturaPows, we are emitting jobs as if we were a pool
    ///so we are using the first 16 bit of our 32 bit nonce per client and send a 0 lower 16
    ///bits for the client to iterate over
    fn next(&mut self) -> Option<Self::Item> {
        let mut newblob = self.blob;

           // Example: upper 16 bits already set for client
        let mut client =  u16::wrapping_add(u16::from_le_bytes(newblob[72..74].try_into().unwrap()), 1);

        // Iterate lower 16 bits

        newblob[72..74].copy_from_slice(&client.to_le_bytes());
        newblob[74..].copy_from_slice(&[0u8;2]);
        self.blob = newblob.clone();
        
        
        Some(DaturaPow {
            seed_hash: self.seed_hash.clone(),
            blob: newblob,
            job_id: self.job_id.clone(),
            target: self.target,
            nonce: 0,
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
            target: u64::from_str_radix(&workOrder.target,16)?,
            nonce: 0
        })
    }
}

impl DaturaPow {

    pub fn next_nonce(&mut self) -> Self {
        let mut newblob = self.blob;
        let solver_nonce = u16::wrapping_add(u16::from_le_bytes(newblob[74..].try_into().unwrap()) , 1);

        newblob[74..].copy_from_slice(&solver_nonce.to_le_bytes());

        self.blob = newblob.clone();
        self.nonce = solver_nonce;

        DaturaPow {
            seed_hash: self.seed_hash.clone(),
            blob: newblob,
            job_id: self.job_id.clone(),
            target: self.target,
            nonce: solver_nonce
        }
    }

    ///Create new Datura pow from p2pool job data
    pub fn new(blob: [u8;76], seed_hash: [u8; 32], job_id: String, target: u64) -> Self {
        DaturaPow {
            blob,
            seed_hash,
            job_id,
            target,
            nonce: 0,
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
            nonce: 0,
        }
    }
}


