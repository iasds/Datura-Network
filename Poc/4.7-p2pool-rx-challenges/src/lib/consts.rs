use std::time::Instant;

pub const NONCE_OFFSET: usize = 39;
pub const NONCE_SIZE: usize = 4;
pub const EXPECTED_DELIVERY_LATENCY: Instant::Duration = Duration::from_millis(500);
