use std::collections::HashMap;
use super::resource::ResourceType;

#[derive(Debug)]
pub struct Allocation {
    current_allocation: u64,
    projected_allocation: u64,
}

#[derive(Debug)]
pub struct Consumer {
    pub allocations: HashMap<ResourceType,Allocation>,
    pub rung: u64,
}

impl Consumer {
    pub fn new() -> Self {
        Consumer {
            allocations: HashMap::new(),
            rung: 0,
        }
    }
}

