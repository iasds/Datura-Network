use randomx_rs::RandomXError;
use thiserror::Error;

#[derive(Error,Debug)]
pub enum WorkerError {
    #[error("invalid configuration, cache or dataset required")]
    InvalidConfiguration,
    #[error("error intiializing randomXVM")]
    RandomXError(#[from]RandomXError),
    #[error("invalid share submitted")]
    InvalidShare,
    #[error("hash solved is too low difficulty")]
    LowDifficultyShare,
}


