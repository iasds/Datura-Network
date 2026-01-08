//! All the errors when interacting with the orderbook

use thiserror::Error;

#[derive(Error,Debug)]
pub enum OrderError {
    ///submitted bid is lower than the price floor
    #[error("submitted bid is lower than the price floor")]
    BidTooLow(u64),
}
