use std::collections::HashMap;
use super::resource::ResourceType;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct Allocation {
    current_allocation: u64,
    projected_allocation: u64,
}

#[derive(Debug)]
pub struct Consumer {
    pub allocations: HashMap<ResourceType,Allocation>,
    pub rung: u64,
    pub allocation_sender: mpsc::Sender<Allocation>,
    pub id: u64,
}

impl Consumer {
    pub fn new() -> (Self,mpsc::Receiver<Allocation>) {
        let (allocation_sender, receiver) = mpsc::channel();
        (Consumer {
            allocations: HashMap::new(),
            rung: 0,
            allocation_sender,
            alive: AtmocBool::new(true),
        }, receiver)
    }
}

