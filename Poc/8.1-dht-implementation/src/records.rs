use serde::{Serialize, Deserialize};

use crate::identity::NodeId;


#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NatRecord {
    pub owner: NodeId,
    pub gateway: NodeId,
    pub external_address: String,
    pub timestamp: u64,
}


#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HSRecord {
    pub hs_hash: NodeId,
    pub rendezvous: NodeId,
    pub expires: u64,
}


#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Record {
    Nat(NatRecord),
    HiddenService(HSRecord),
}