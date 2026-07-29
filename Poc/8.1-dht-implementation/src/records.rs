use serde::{Deserialize, Serialize};

use crate::identity::NodeId;

/// A record for pointing a node behind NAT toward a reachable gateway.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NatRecord {
    pub owner: NodeId,
    pub gateway: NodeId,
    pub external_address: String,
    pub timestamp: u64,
}

/// A descriptor for a hidden service.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HSRecord {
    pub hidden_service_hash: NodeId,
    pub rendezvous: NodeId,
    pub expires: u64,
}

/// A wrapper NAT and HS records.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Record {
    Nat(NatRecord),
    HiddenService(HSRecord),
}
