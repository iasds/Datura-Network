use std::time::Duration;

pub const NONCE_OFFSET: usize = 39;
pub const NONCE_SIZE: usize = 4;
pub const EXPECTED_DELIVERY_LATENCY: Duration = Duration::from_millis(500);
pub const SOLVER_CHANNEL_SIZE: usize = 64;
pub const WORKER_CHANNEL_SIZE: usize = 16;
