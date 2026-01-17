use tokio::time::Duration;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use leaky_bucket::RateLimiter;
use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

const DATA_ADDR: &str = "127.0.0.1:9977";
const CONTROL_ADDR: &str = "127.0.0.1:9978";
const BUFFER_SIZE: usize = 4096;

const DEFAULT_BANDWIDTH: usize = 10 * 1024; // 10kb
const CHALLENGE_DIFFICULTY: u8 = 6;

type NodeID = IpAddr;  // node are identified by their ip address

// copied from Pow-4.
fn create_challenge() -> [u8; 16] {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).unwrap();
    buf[0] = CHALLENGE_DIFFICULTY & 0b111111;

    buf
}

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

async fn control_thread(
    tx: mpsc::Sender<(NodeID, [u8; 16], [u8; 8], oneshot::Sender<bool>)>,
    limiters: Arc<Mutex<HashMap<NodeID, Arc<RateLimiter>>>>
) -> Result<(), Box<dyn Error>>  {
    let listener = TcpListener::bind(CONTROL_ADDR).await?;

    loop {
	let (mut socket, addr) = listener.accept().await.unwrap();
	let tx = tx.clone();

	tokio::spawn(async move {
	    let mut buf = [0; 8];

	    // new challenge
	    let challenge = create_challenge();
	    socket.write_all(&challenge).await.unwrap();

	    loop {
		if socket.read(&mut buf).await.unwrap() == 8 {
		    let mut solution = [0; 8];
		    solution.copy_from_slice(&buf[0..8]);

		    let (vm_tx, vm_rx) = oneshot::channel::<bool>();
		    tx.send((addr.ip(), challenge, solution, vm_tx)).await.unwrap();
		    if vm_rx.await.unwrap() {
			todo!("blablabla randomx");
		    }
		}
	    }
	});
    }
}

#[tokio::main]
async fn main() {
    let limiters: Arc<Mutex<HashMap<NodeID, Arc<RateLimiter>>>> = Arc::new(Mutex::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel(4096);

    tokio::select! {
	_ = data_thread(limiters.clone()) => {},
	_ = control_thread(tx, limiters.clone()) => {},
    };
}
