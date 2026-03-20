//! The client uploads a file given to the command-line.
use std::env;
use std::error::Error;
use tokio::fs::File;

const CONTROL_ADDR: &str = "127.0.0.1:9978";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let fname: String = env::args().next().unwrap();
	let file = File::open(fname).await?;

	let metadata = file.metadata().await?;

	println!("{:?}", metadata);
	Ok(())
}
