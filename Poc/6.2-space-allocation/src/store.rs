use rand::Rng;
use tokio::io;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

const DATA_PATH: &str = "/tmp/datura/store";
const BUFFER_SIZE: usize = 8192;

// inspired from https://github.com/tokio-rs/tokio/blob/master/examples/echo-tcp.rs
pub async fn read_from(socket: &mut TcpStream, mut data_len: usize) -> io::Result<[u8; 32]> {
	let mut id = [0u8; 32];
	rand::rng().fill(&mut id);

	let mut buf = vec![0; BUFFER_SIZE];

	while data_len > 0 {
		match socket.read(&mut buf).await {
			// Connection closed by peer
			Ok(0) => return Ok(id),
			Ok(n) => {
				todo!();
			}
			Err(e) => return Err(e),
		}
	}
	Ok(id)
}
