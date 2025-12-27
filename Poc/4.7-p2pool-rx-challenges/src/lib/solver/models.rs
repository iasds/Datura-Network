use rand::fill;
use super::errors::SolverError;

///Structure for datura network proof of work, can be autonomously generated
///or created from a p2pool job
pub struct DaturaPow {
    blob: [u8;128],
    seed_hash: [u8;32],
    job_id: String,
}

impl TryFrom<ServerReply> for DaturaPow {
    fn try_from(workOrker: ServerReply) -> Result<DaturaPow,SolverError> {
        match workOrder {
            LoginReply { id, result, .. } => {
                Ok(DaturaPow {
                    job_id: result.id,
                    blob:
                
            }
        }
    }
}

impl DaturaPow {
    pub fn new(blob: [u8:128], seed_hash [u8: 32], job_id: String) -> Self {
        ///Create new Datura pow from p2pool job data
        DaturaPow {
            blob,
            seed_hash,
            job_id,
        }
    }
    pub fn random() -> Self {
        ///generate really random challenge
        let mut blob: [0u8;128];
        let mut seed_hash: [0u8;32];

        fill(&mut blob);
        fill(&mut seed_hash);
        DaturaPow {
            job_id: "0".to_string(),
            blob,
            seed_hash,
        }
    }
}


