//! The client only tries to solve the RandomX challenge.
use space_allocation::pow;
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	net::TcpStream,
};

const CONTROL_ADDR: &str = "127.0.0.1:9978";

#[tokio::main]
async fn main() {
	let mut solution = rand::random::<u64>();

	let vm = pow::create_vm().unwrap();

	let mut challenge = [0; 16];

	let mut stream = TcpStream::connect(CONTROL_ADDR).await.unwrap();
	match stream.read(&mut challenge).await {
		Ok(16) => {
			println!("Challenge received.");
		}
		_ => {
			eprintln!("Server closed connection unexpectedly.");
			return;
		}
	}
	// do not hold the connection. the control thread holds a deadlock on your node's
	// RateLimiter, so you can't send simultaneously data on the control and the data
	// port. it only affects your node, e.g you can't block other nodes. should this be
	// fixed?
	stream.shutdown().await.unwrap();
	println!("Difficulty is {}.", challenge[0] & 0b111111);

	while !pow::validate_solution(&vm, challenge, solution.to_ne_bytes()) {
		solution += 1;
	}

	let mut stream = TcpStream::connect(CONTROL_ADDR).await.unwrap();
	match stream.write_all(&solution.to_ne_bytes()).await {
		Ok(_) => {
			println!("Challenge has been solved, and throttling lifted.");
		}
		Err(e) => {
			eprintln!("Unexpected error: {}", e);
		}
	}
}
