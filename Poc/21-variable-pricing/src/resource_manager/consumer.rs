use std::collections::HashMap;
use super::resource::ResourceType;

#[derive(Debug, Clone)]
pub struct Allocation {
    pub current_allocation: u64,
    pub projected_allocation: u64,
}

#[derive(Debug, Clone)]
pub struct Consumer {
    pub allocations: HashMap<ResourceType, Allocation>,
    pub id: u64,
    pub contribution: u64,
    pub epochs_connected: u32,
    pub allocation: f64,
}

impl Consumer {
    pub fn new(id: u64, contribution: u64, epochs_connected: u32) -> Self {
        Consumer {
            allocations: HashMap::new(),
            id,
            contribution,
            epochs_connected,
            allocation: 0.0,
        }
    }
}
