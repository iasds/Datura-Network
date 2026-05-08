use crate::bandwidth::NodeRateLimiter;
use fs2::available_space;
use rand::Rng;
use std::sync::Arc;
use tokio::fs;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

const DATA_PATH: &str = "/tmp/datura/store";
const BUFFER_SIZE: usize = 8192;

pub async fn init() -> io::Result<()> {
	fs::create_dir_all(DATA_PATH).await
}

pub fn check_free_space(n: usize) -> bool {
	match available_space(DATA_PATH) {
		Ok(f) => f >= (n as u64),
		Err(_) => false,
	}
}

pub fn difficulty(n: usize) -> u8 {
	n.ilog10().max(3) as u8 - 3
}

pub async fn retrieve(dataid: [u8; 32]) -> io::Result<fs::File> {
	fs::File::open(format!("{}/{}", DATA_PATH, const_hex::encode(dataid))).await
}

// inspired from https://github.com/tokio-rs/tokio/blob/master/examples/echo-tcp.rs
pub async fn read_from(
	socket: &mut TcpStream,
	mut n: usize,
	limiter: Arc<Mutex<NodeRateLimiter>>,
) -> io::Result<[u8; 32]> {
	let mut id = [0u8; 32];
	rand::rng().fill(&mut id);

	let mut buf = vec![0; BUFFER_SIZE];
	let mut file = fs::File::create(format!("{}/{}", DATA_PATH, const_hex::encode(id))).await?;

	while n > 0 {
		match socket.read(&mut buf).await {
			// Connection closed by peer
			Ok(0) => return Ok(id),
			Ok(b) => {
				let len = b.min(n);
				file.write_all(&buf[0..len]).await?;
				n -= len;
				limiter.lock().await.bucket.acquire(len).await;
			}
			Err(e) => return Err(e),
		}
	}
	Ok(id)
}
