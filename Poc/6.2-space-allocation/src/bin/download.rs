use std::env;
use std::error::Error;
use tokio::io;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

const CONTROL_ADDR: &str = "127.0.0.1:9978";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let dataid = const_hex::decode(env::args().skip(1).next().unwrap()).unwrap();
	let mut stream = TcpStream::connect(CONTROL_ADDR).await?;

	stream.write("GET ".as_bytes()).await?;
	stream.write_all(&dataid).await?;

	io::copy(&mut stream, &mut io::stdout()).await?;

	Ok(())
}
