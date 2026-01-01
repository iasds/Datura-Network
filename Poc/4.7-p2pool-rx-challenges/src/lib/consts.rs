use std::time::Duration;

pub const NONCE_OFFSET: usize = 39;
pub const NONCE_SIZE: usize = 4;
pub const SOLVER_CHANNEL_SIZE: usize = 64;
pub const WORKER_CHANNEL_SIZE: usize = 16;
pub const VERIFY_USUAL_DURATION: Duration = Duration::from_millis(50);

/// pow max lifetime, for random pows they will have an expiration between 0 and this lifetime
/// client job list is also cleaned up based on this lifetime
pub const POW_MAX_LIFETIME: Duration = Duration::from_secs(3);

/// random seed_hash max lifetime when used for genrating new pows
pub const SEED_LIFETIME: Duration = Duration::from_hours(48);

/// minimal difficulty for new client pows
pub const MINIMAL_DIFFICULTY: u64 = 200; //to test
