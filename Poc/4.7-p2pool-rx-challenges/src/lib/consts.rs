//! general constants used for running the solver and client modules

use std::time::Duration;

///nonce position inside a job blob (don't touch unless you know what you're doing)
pub const NONCE_OFFSET: usize = 39;

///nonce size (accounting for the fact you might be using xmrig-proxy)
pub const NONCE_SIZE: usize = 4;

///number of items to keep in the solver input channels
pub const SOLVER_CHANNEL_SIZE: usize = 64;

///number of items to keep in each thread worker input channel
pub const WORKER_CHANNEL_SIZE: usize = 16;

///estimation of usual verification timing (unused for now)
pub const VERIFY_USUAL_DURATION: Duration = Duration::from_millis(50);

/// pow max lifetime, for random pows they will have an expiration between 0 and this lifetime
/// client job list is also cleaned up based on this lifetime
pub const POW_MAX_LIFETIME: Duration = Duration::from_secs(3);

/// random seed_hash max lifetime when used for genrating new pows
pub const SEED_LIFETIME: Duration = Duration::from_hours(48);

/// minimal difficulty for new client pows
pub const MINIMAL_DIFFICULTY: u64 = 200; //to test
