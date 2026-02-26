// ============================================================================
// Epoch Configuration
// ============================================================================

/// Duration of each allocation epoch in seconds
pub const EPOCH_DURATION_SECS: u64 = 60;

/// Number of grace epochs for new connections before activity enforcement
/// New clients get this many epochs to submit their first qualifying shares
pub const ONBOARDING_GRACE_EPOCHS: u32 = 1;

// ============================================================================
// Connection Requirements
// ============================================================================

/// Initial proof-of-work difficulty required to establish connection
/// 2^14 = 16,384 difficulty (~1.6s @ 10 KH/s)
pub const CONNECTION_POW_DIFFICULTY: u64 = 16_384;

/// Minimum number of valid shares required per epoch to maintain connection
/// Applied after grace period expires
pub const MIN_SHARES_PER_EPOCH: u32 = 1;

/// Minimum contribution threshold to qualify for minimum allocation guarantee
/// Approximately 5 KH/s mining for 60 seconds at difficulty 2^8 base
/// Prevents Sybil attacks via minimal-effort clients
pub const MIN_CONTRIBUTION_THRESHOLD: u64 = 50_000;

// ============================================================================
// Allocation Algorithm Parameters
// ============================================================================

/// Sublinear compression exponent for contribution-based allocation
/// Contributions are raised to this power before proportional distribution
/// Range: 0.5 (aggressive compression) to 0.8 (mild compression)
/// 0.55 provides strong diminishing returns: 10× hashrate → ~4× allocation
pub const COMPRESSION_EXPONENT: f64 = 0.5;

/// Maximum percentage of total capacity reserved for minimum allocations
/// Total of all minimum guarantees cannot exceed this
pub const MAX_RESERVE_PERCENTAGE: f64 = 0.15;

/// Reserve percentage earned per epoch of tenure
/// Caps at MAX_TENURE_EPOCHS
/// New client: 0× reserve, 10-epoch client: 10 × 0.015 = 0.15 (15%)
pub const RESERVE_PER_EPOCH_TENURE: f64 = 0.015;

/// Maximum number of epochs counted for tenure-based minimum calculation
/// Caps exponential growth of minimum allocations for long-lived connections
pub const MAX_TENURE_EPOCHS: u32 = 10;

// ============================================================================
// Share Validation
// ============================================================================

/// Minimum p2pool difficulty for shares to be accepted
/// Shares below this are rejected regardless of mining target
pub const P2POOL_MIN_DIFFICULTY: u64 = 256; // 2^8

/// Maximum percentage of total capacity any single client can receive
/// Prevents monopolization even with extreme hashrate
pub const MAX_ALLOCATION_PER_CLIENT_PERCENTAGE: f64 = 0.25;

/// Absolute minimum allocation per qualified client
/// Expressed as percentage of total capacity
/// Only applies to clients meeting MIN_CONTRIBUTION_THRESHOLD
pub const ABSOLUTE_MIN_ALLOCATION_PERCENTAGE: f64 = 0.001;
