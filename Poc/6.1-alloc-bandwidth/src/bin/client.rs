//! The client only tries to solve the RandomX challenge.
use alloc_bandwidth::pow;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const CONTROL_ADDR: &str = "127.0.0.1:9978";

#[tokio::main]
async fn main() {
    let mut stream = TcpStream::connect(CONTROL_ADDR).await.unwrap();
    let mut solution = getrandom::u64().unwrap();

    let vm = pow::create_vm().unwrap();

    let mut challenge = [0; 16];

    match stream.read(&mut challenge).await {
        Ok(16) => {
            println!("Challenge received.");
            while !pow::validate_solution(&vm, challenge, solution.to_ne_bytes()) {
                solution += 1;
            }
            match stream.write_all(&solution.to_ne_bytes()).await {
                Ok(_) => {
                    println!("Challenge has been solved, and throttling lifted.");
                }
                Err(e) => {
                    eprintln!("Unexpected error: {}", e);
                }
            }
        }
        _ => {
            eprintln!("Unexpected server behavior.");
        }
    }
}
