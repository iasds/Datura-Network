#[derive(Debug, Clone)]
pub struct Consumer {
    pub id: u64,
    pub contribution: u64,
    pub epochs_connected: u32,
    pub allocation: f64,
}

impl Consumer {
    pub fn new(id: u64, contribution: u64, epochs_connected: u32) -> Self {
        Consumer {
            id,
            contribution,
            epochs_connected,
            allocation: 0.0,
        }
    }
}
