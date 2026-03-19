use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::Instant;

use space_allocation::bandwidth::{
	AUTH_BANDWIDTH, NODE_BANDWIDTH, NodeRate, NodeRateLimiter, TOTAL_BANDWIDTH_LIMITER, difficulty,
};
use space_allocation::pow;
use space_allocation::protocol::Protocol;

const DATA_ADDR: &str = "127.0.0.1:9977";
const CONTROL_ADDR: &str = "127.0.0.1:9978";
const BUFFER_SIZE: usize = 8192;

type NodeID = IpAddr; // node are identified by their ip address

type NodeHashMap = Arc<Mutex<HashMap<NodeID, Arc<Mutex<NodeRateLimiter>>>>>;

// inspired from https://github.com/tokio-rs/tokio/blob/master/examples/echo-tcp.rs
async fn data_thread(limiters: NodeHashMap) -> Result<(), Box<dyn Error>> {
	let listener = TcpListener::bind(DATA_ADDR).await?;

	loop {
		let (mut socket, addr) = listener.accept().await.unwrap();
		let mut stdout = tokio::io::stdout();
		let limiters = limiters.clone();

		tokio::spawn(async move {
			let mut buf = vec![0; BUFFER_SIZE];

			loop {
				match socket.read(&mut buf).await {
					Ok(0) => {
						return;
					}
					Ok(n) => {
						// write to the standard output. if writing fails, log and exit.
						if let Err(e) = stdout.write_all(&buf[0..n]).await {
							eprintln!("Failed to write to socket {}: {}", addr, e);
							return;
						}

						let limiter = limiters
							.lock()
							.await
							.entry(addr.ip())
							.or_insert_with(|| Arc::new(Mutex::new(NodeRateLimiter::anon())))
							.clone();

						let mut limiter = limiter.lock().await;

						match limiter.rate {
							NodeRate::Auth(timeout, cap) => {
								if cap > n && timeout > Instant::now() {
									limiter.rate = NodeRate::Auth(timeout, cap - n);
								} else {
									*limiter = NodeRateLimiter::anon();
								}
							}
							NodeRate::Anon(_) => (),
						};

						limiter.bucket.acquire(n).await;
						// we don't want the scheduler to lock if we go >100%.
						TOTAL_BANDWIDTH_LIMITER.lock().await.try_acquire(n);
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
	limiters: NodeHashMap,
) -> Result<(), Box<dyn Error>> {
	let listener = TcpListener::bind(CONTROL_ADDR).await?;

	loop {
		let (mut socket, addr) = listener.accept().await?;
		let tx = tx.clone();
		let mut instruction: [u8; 32] = [0; 32];
		let len = socket.read(&mut instruction).await?;
		match str::from_utf8(&instruction[..len])
			.map_err(|_| ())
			.and_then(Protocol::from_str)
		{
			Ok(Protocol::Knock) => {
				let limiters = limiters.clone();
				tokio::spawn(async move {
					let mut solution = [0; 8];
					let limiter = limiters
						.lock()
						.await
						.entry(addr.ip())
						.or_insert_with(|| Arc::new(Mutex::new(NodeRateLimiter::anon())))
						.clone();

					let mut limiter = limiter.lock().await;

					match &mut limiter.rate {
						NodeRate::Anon(challenge) => {
							let challenge = challenge.get(difficulty().await);
							socket.write_all(&challenge).await.unwrap();

							if socket.read(&mut solution).await.unwrap() == 8 {
								let (vm_tx, vm_rx) = oneshot::channel::<bool>();
								tx.send((addr.ip(), challenge, solution, vm_tx))
									.await
									.unwrap();
								if vm_rx.await.unwrap() {
									*limiter = NodeRateLimiter::auth();
								}
							} else {
								// This is very bad for performance, obviously.
								eprintln!(
									"Requested {} of bandwith from {} available.",
									AUTH_BANDWIDTH,
									{
										let node = TOTAL_BANDWIDTH_LIMITER.lock().await;
										NODE_BANDWIDTH as isize
											- (node.max() - node.balance()) as isize
									}
								);
							}
						}
						NodeRate::Auth(..) => (),
					}
				});
			}
			Ok(Protocol::Put(n)) => {
				let limiter = limiters
					.lock()
					.await
					.entry(addr.ip())
					.or_insert_with(|| Arc::new(Mutex::new(NodeRateLimiter::anon())))
					.clone();

				let mut limiter = limiter.lock().await;
				match &mut limiter.rate {
					NodeRate::Anon(..) => {}
					NodeRate::Auth(..) => {
						todo!()
					}
				}
			}
			Ok(Protocol::Get(_)) => todo!(),
			Err(_) => {}
		}
	}
}

#[tokio::main]
async fn main() {
	let limiters: NodeHashMap = Arc::new(Mutex::new(HashMap::new()));
	let (tx, mut rx) = mpsc::channel::<(IpAddr, [u8; 16], [u8; 8], oneshot::Sender<bool>)>(4096);

	let vm_thread = thread::spawn(move || {
		let vm = pow::create_vm().unwrap();

		while let Some((_node_id, challenge, solution, msg)) = rx.blocking_recv() {
			msg.send(pow::validate_solution(&vm, challenge, solution))
				.unwrap();
		}
	});

	tokio::select! {
		_ = data_thread(limiters.clone()) => {},
		_ = control_thread(tx, limiters.clone()) => {},
	};

	vm_thread.join().unwrap();
}
