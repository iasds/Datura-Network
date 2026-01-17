use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::env;
use std::error::Error;

const DEFAULT_ADDR: &str = "127.0.0.1";
const DATA_PORT: &str = "9977";
const BUFFER_SIZE: usize = 4096;


// inspired from https://github.com/tokio-rs/tokio/blob/master/examples/echo-tcp.rs
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ADDR.to_string()) + ":" + DATA_PORT;

    let listener = TcpListener::bind(&addr).await?;

    loop {
        let (mut socket, addr) = listener.accept().await?;
	let mut stdout = tokio::io::stdout();

        tokio::spawn(async move {
            let mut buf = vec![0; BUFFER_SIZE];

            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => {
			// closed by peer
                        return;
                    }
                    Ok(n) => {
			// write to the standard output. if writing fails, log and exit.
                        if let Err(e) = stdout.write_all(&buf[0..n]).await {
                            eprintln!("Failed to write to socket {}: {}", addr, e);
                            return;
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read from socket {}: {}", addr, e);
                        return;
                    }
                }
            }
        });
    }
}
