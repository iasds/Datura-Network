use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

mod client;
mod stratum_models;
mod solver;
pub use client::Client;
