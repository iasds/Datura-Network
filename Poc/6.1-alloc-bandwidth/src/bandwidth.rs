use std::sync::LazyLock;
use tokio::{
    sync::Mutex,
    time::{Duration, Instant},
};

use leaky_bucket::RateLimiter;

use crate::pow::Challenge;

const ANON_BANDWIDTH: usize = 10 * 1024; // 10kb
const AUTH_BANDWIDTH: usize = 1024 * 1024; // 1mb
const DATA_CAP: usize = 100 * 1024 * 1024; // 100mb
const TIME_CAP: u64 = 1; // 1h

#[derive(Debug, Clone)]
pub enum NodeRate {
    Anon(Challenge),
    Auth(Instant, usize),
}

pub struct NodeRateLimiter {
    pub bucket: RateLimiter,
    pub rate: NodeRate,
}

impl NodeRateLimiter {
    pub fn anon() -> Self {
        Self {
            bucket: RateLimiter::builder()
                .initial(ANON_BANDWIDTH)
                .max(ANON_BANDWIDTH)
                .refill(ANON_BANDWIDTH / 100)
                .interval(Duration::from_millis(10))
                .build(),
            rate: NodeRate::Anon(Challenge::new()),
        }
    }

    pub fn auth() -> Self {
        Self {
            bucket: RateLimiter::builder()
                .initial(AUTH_BANDWIDTH)
                .max(AUTH_BANDWIDTH)
                .refill(AUTH_BANDWIDTH / 100)
                .interval(Duration::from_millis(10))
                .build(),
            rate: NodeRate::Auth(Instant::now() + Duration::from_hours(TIME_CAP), DATA_CAP),
        }
    }
}
