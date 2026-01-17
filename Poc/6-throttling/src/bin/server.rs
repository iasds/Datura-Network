use tokio::time::Duration;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use leaky_bucket::RateLimiter;
use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

const DATA_ADDR: &str = "127.0.0.1:9977";
const BUFFER_SIZE: usize = 4096;

const DEFAULT_BANDWIDTH: usize = 10 * 1024; // 10kb

type NodeID = IpAddr;  // node are identified by their ip address

// inspired from https://github.com/tokio-rs/tokio/blob/master/examples/echo-tcp.rs
async fn data_thread(
    limiters: Arc<Mutex<HashMap<NodeID, Arc<RateLimiter>>>>
) -> Result<(), Box<dyn Error>>  {
    let listener = TcpListener::bind(DATA_ADDR).await?;

    loop {
	let (mut socket, addr) = listener.accept().await.unwrap();
	let mut stdout = tokio::io::stdout();
	let limiters = limiters.clone();

	tokio::spawn(async move {
	    let mut buf = vec![0; BUFFER_SIZE];

	    loop {
		match socket.read(&mut buf).await {
		    Ok(0) => { return; }
		    Ok(n) => {
			// write to the standard output. if writing fails, log and exit.
			if let Err(e) = stdout.write_all(&buf[0..n]).await {
			    eprintln!("Failed to write to socket {}: {}", addr, e);
			    return;
			}
			let limiter = limiters
			    .lock()
			    .unwrap()
			    .entry(addr.ip())
			    .or_insert_with(|| {
				// 10kb rate limiter builder for current IP.
				Arc::new(
				    RateLimiter::builder()
					.initial(DEFAULT_BANDWIDTH)
					.max(DEFAULT_BANDWIDTH)
					.refill(DEFAULT_BANDWIDTH / 100)
					.interval(Duration::from_millis(10))
					.build()
				)
			    })
			    .clone();

			limiter.acquire(n).await;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let limiters: Arc<Mutex<HashMap<NodeID, Arc<RateLimiter>>>> = Arc::new(Mutex::new(HashMap::new()));

    data_thread(limiters).await
}
