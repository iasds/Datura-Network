//! The client only tries to solve the RandomX challenge.
use std::io::ErrorKind;

use tokio::{io::AsyncWriteExt, net::TcpStream};

mod pow;

const CONTROL_ADDR: &str = "127.0.0.1:9978";

#[tokio::main]
async fn main() {
    let mut stream = TcpStream::connect(CONTROL_ADDR).await.unwrap();
    let mut solution = getrandom::u64().unwrap();

    loop {
        let bytes = solution.to_ne_bytes();

        match stream.write_all(&bytes).await {
            Ok(_) => {
                solution += 1;
            }
            Err(e) => {
		if e.kind() == ErrorKind::ConnectionReset {
		    println!("Challenge has been solved, and throttling lifted.");
		} else {
		    eprintln!("Unexpected error: {}", e);
		}
                break;
            }
        }
    }
}
