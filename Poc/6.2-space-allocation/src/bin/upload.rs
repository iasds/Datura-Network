//! The client uploads a file given to the command-line.
use std::env;
use std::error::Error;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

const CONTROL_ADDR: &str = "127.0.0.1:9978";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let fname: String = env::args().next().unwrap();
	let file = File::open(fname).await?;
	let file_size = file.metadata().await?.len();

	let mut stream = TcpStream::connect(CONTROL_ADDR).await.unwrap();

	stream
		.write_all(format!("PUT {}", file_size).as_bytes())
		.await?;

	Ok(())
}
