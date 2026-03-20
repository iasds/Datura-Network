//! The client uploads a file given to the command-line.
use space_allocation::pow;
use std::env;
use std::error::Error;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

const CONTROL_ADDR: &str = "127.0.0.1:9978";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let fname: String = env::args().next().unwrap();
	let file = File::open(fname).await?;
	let file_size = file.metadata().await?.len();

	let mut stream = TcpStream::connect(CONTROL_ADDR).await.unwrap();

	let mut challenge = [0; 16];
	let vm = pow::create_vm().unwrap();
	let mut solution = rand::random::<u64>();

	stream
		.write_all(format!("PUT {}", file_size).as_bytes())
		.await?;

	stream.read_exact(&mut challenge).await?;

	// do not hold the connection. the control thread holds a deadlock on your node's
	// RateLimiter, so you can't send simultaneously data on the control and the data
	// port. it only affects your node, e.g you can't block other nodes. should this be
	// fixed?
	stream.shutdown().await?;
	println!("Difficulty is {}.", challenge[0] & 0b111111);

	while !pow::validate_solution(&vm, challenge, solution.to_ne_bytes()) {
		solution += 1;
	}

	Ok(())
}
