use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::LazyLock;
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
use space_allocation::store;

const DATA_ADDR: &str = "127.0.0.1:9977";
const CONTROL_ADDR: &str = "127.0.0.1:9978";
const BUFFER_SIZE: usize = 8192;

type NodeID = IpAddr; // node are identified by their ip address

static RATE_LIMITERS: LazyLock<Mutex<HashMap<NodeID, Arc<Mutex<NodeRateLimiter>>>>> =
	LazyLock::new(|| Mutex::new(HashMap::new()));

static STORE_CHALLENGES: LazyLock<Mutex<HashMap<(NodeID, usize), pow::Challenge>>> =
	LazyLock::new(|| Mutex::new(HashMap::new()));

async fn data_thread() -> Result<(), Box<dyn Error>> {
	let listener = TcpListener::bind(DATA_ADDR).await?;

	loop {
		let (mut socket, addr) = listener.accept().await.unwrap();
		let mut stdout = tokio::io::stdout();

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

						let limiter = RATE_LIMITERS
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
	tx: mpsc::Sender<([u8; 16], [u8; 8], oneshot::Sender<bool>)>,
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
				tokio::spawn(async move {
					let mut solution = [0; 8];
					let limiter = RATE_LIMITERS
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
								tx.send((challenge, solution, vm_tx)).await.unwrap();
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
				tokio::spawn(async move {
					let challenge = STORE_CHALLENGES
						.lock()
						.await
						.entry((addr.ip(), n))
						.or_default()
						.get(store::difficulty(n));

					socket.write_all(&challenge).await.unwrap();
					let mut solution = [0; 8];

					if socket.read(&mut solution).await.unwrap() != 8 {
						eprintln!("Requested to store {} bytes.", n);
						return;
					};

					let (vm_tx, vm_rx) = oneshot::channel::<bool>();
					tx.send((challenge, solution, vm_tx)).await.unwrap();
					if vm_rx.await.unwrap() && store::check_free_space(n) {
						let limiter = RATE_LIMITERS
							.lock()
							.await
							.entry(addr.ip())
							.or_insert_with(|| Arc::new(Mutex::new(NodeRateLimiter::anon())))
							.clone();

						if let Ok(id) = store::read_from(&mut socket, n, limiter).await {
							socket.write(&id).await.unwrap();
							STORE_CHALLENGES.lock().await.remove(&(addr.ip(), n));
						};
					}
				});
			}
			Ok(Protocol::Get(_)) => todo!(),
			Err(_) => {}
		}
	}
}

#[tokio::main]
async fn main() {
	let (tx, mut rx) = mpsc::channel::<([u8; 16], [u8; 8], oneshot::Sender<bool>)>(4096);

	let vm_thread = thread::spawn(move || {
		let vm = pow::create_vm().unwrap();

		while let Some((challenge, solution, msg)) = rx.blocking_recv() {
			msg.send(pow::validate_solution(&vm, challenge, solution))
				.unwrap();
		}
	});

	store::init().await.unwrap();

	tokio::select! {
		_ = data_thread() => {},
		_ = control_thread(tx) => {},
	};

	vm_thread.join().unwrap();
}
