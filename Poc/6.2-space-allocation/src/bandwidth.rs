use std::sync::LazyLock;
use tokio::{
	sync::Mutex,
	time::{Duration, Instant},
};

use leaky_bucket::RateLimiter;

use crate::pow::Challenge;

pub const ANON_BANDWIDTH: usize = 10 * 1024; // 10kb
pub const AUTH_BANDWIDTH: usize = 1024 * 1024; // 1mb
pub const NODE_BANDWIDTH: usize = 100 * 1024 * 1024; // 100mb
const DATA_CAP: usize = 100 * 1024 * 1024; // 100mb
const TIME_CAP: u64 = 1; // 1h

const NORMAL_DIFFICULTY: u8 = 12; // standard difficulty of the challenge

pub static TOTAL_BANDWIDTH_LIMITER: LazyLock<Mutex<RateLimiter>> = LazyLock::new(|| {
	Mutex::new(
		RateLimiter::builder()
			.initial(usize::MAX - 1)
			.max(usize::MAX - 1)
			.refill(NODE_BANDWIDTH / 100)
			.interval(Duration::from_millis(10))
			.build(),
	)
});

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

/// Calculate the current difficulty, based on available bandwidth.
///
/// The challenge difficulty increases, following how much bandwidth is currently taken.
/// Until reaching 90% of occupation, it stays at `NORMAL_DIFFICULTY`, then the average
/// computing time of the solution doubles (meaning, difficulty increases of 1) for
/// every 10% of bandwidth.
pub async fn difficulty() -> u8 {
	let used_bandwidth = {
		let node = TOTAL_BANDWIDTH_LIMITER.lock().await;
		node.max() - node.balance()
	};

	let difficulty_increase = (used_bandwidth as f64 / NODE_BANDWIDTH as f64 - 0.9).max(0.0) * 10.0;

	(difficulty_increase + NORMAL_DIFFICULTY as f64).round() as u8
}
