use rand::Rng;
use tokio::fs;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const DATA_PATH: &str = "/tmp/datura/store";
const BUFFER_SIZE: usize = 8192;

pub async fn init() -> io::Result<()> {
	fs::create_dir_all(DATA_PATH).await
}

// inspired from https://github.com/tokio-rs/tokio/blob/master/examples/echo-tcp.rs
pub async fn read_from(socket: &mut TcpStream, mut data_len: usize) -> io::Result<[u8; 32]> {
	let mut id = [0u8; 32];
	rand::rng().fill(&mut id);

	let mut buf = vec![0; BUFFER_SIZE];
	let mut file = fs::File::create(format!("{}/{}", DATA_PATH, const_hex::encode(id))).await?;

	while data_len > 0 {
		match socket.read(&mut buf).await {
			// Connection closed by peer
			Ok(0) => return Ok(id),
			Ok(n) => {
				let len = n.min(data_len);
				file.write_all(&buf[0..len]).await?;
				data_len -= len;
			}
			Err(e) => return Err(e),
		}
	}
	Ok(id)
}
