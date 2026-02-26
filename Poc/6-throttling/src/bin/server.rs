use leaky_bucket::RateLimiter;
use randomx_rs::{RandomXCache, RandomXDataset, RandomXFlag, RandomXVM};
use std::collections::HashMap;
use std::error::Error;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio::time::sleep;

use throttling::pow;

const DATA_ADDR: &str = "127.0.0.1:9977";
const CONTROL_ADDR: &str = "127.0.0.1:9978";
const BUFFER_SIZE: usize = 8192;

const DEFAULT_BANDWIDTH: usize = 10 * 1024; // 10kb
const VALIDATED_BANDWIDTH: usize = 1024 * 1024; // 1mb
const DATA_CAP: usize = 100 * 1024 * 1024; // 100mb
const TIME_CAP: u64 = 3600; // 1h

type NodeID = IpAddr; // node are identified by their ip address
type NodeHashMap =
    Arc<Mutex<HashMap<NodeID, (Arc<RateLimiter>, Option<(Arc<JoinHandle<()>>, usize)>)>>>;

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
                        let (limiter, _) = limiters
                            .lock()
                            .unwrap()
                            .entry(addr.ip())
                            .and_modify(|(limiter, cancellations)| {
                                let finished = cancellations
                                    .as_mut()
                                    .map(|(timeout, cap)| {
                                        if *cap > n {
                                            *cap -= n;
                                            false
                                        } else {
                                            timeout.abort();
                                            true
                                        }
                                    })
                                    .unwrap_or(false);

                                if finished {
                                    *limiter = Arc::new(
                                        RateLimiter::builder()
                                            .initial(DEFAULT_BANDWIDTH)
                                            .max(DEFAULT_BANDWIDTH)
                                            .refill(DEFAULT_BANDWIDTH / 100)
                                            .interval(Duration::from_millis(10))
                                            .build(),
                                    );
                                    *cancellations = None;
                                }
                            })
                            .or_insert(
                                // 10kb rate limiter builder for current IP.
                                (
                                    Arc::new(
                                        RateLimiter::builder()
                                            .initial(DEFAULT_BANDWIDTH)
                                            .max(DEFAULT_BANDWIDTH)
                                            .refill(DEFAULT_BANDWIDTH / 100)
                                            .interval(Duration::from_millis(10))
                                            .build(),
                                    ),
                                    None,
                                ),
                            )
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
    limiters: NodeHashMap,
) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(CONTROL_ADDR).await?;

    loop {
        let (mut socket, addr) = listener.accept().await.unwrap();
        let tx = tx.clone();

        let limiters = limiters.clone();
        tokio::spawn(async move {
            let mut solution = [0; 8];

            // new challenge
            let challenge = pow::create_challenge();
            socket.write_all(&challenge).await.unwrap();

            if socket.read(&mut solution).await.unwrap() == 8 {
                let limiters_ = limiters.clone();

                let (vm_tx, vm_rx) = oneshot::channel::<bool>();
                tx.send((addr.ip(), challenge, solution, vm_tx))
                    .await
                    .unwrap();
                if vm_rx.await.unwrap() {
                    limiters.lock().unwrap().insert(
                        addr.ip(),
                        (
                            Arc::new(
                                // 1mb rate limiter
                                RateLimiter::builder()
                                    .initial(VALIDATED_BANDWIDTH)
                                    .max(VALIDATED_BANDWIDTH)
                                    .refill(VALIDATED_BANDWIDTH / 100)
                                    .interval(Duration::from_millis(10))
                                    .build(),
                            ),
                            Some((
                                Arc::new(tokio::spawn(async move {
                                    sleep(Duration::from_secs(TIME_CAP)).await;
                                    limiters_.lock().unwrap().insert(
                                        addr.ip(),
                                        (
                                            Arc::new(
                                                RateLimiter::builder()
                                                    .initial(DEFAULT_BANDWIDTH)
                                                    .max(DEFAULT_BANDWIDTH)
                                                    .refill(DEFAULT_BANDWIDTH / 100)
                                                    .interval(Duration::from_millis(10))
                                                    .build(),
                                            ),
                                            None,
                                        ),
                                    );
                                })),
                                DATA_CAP,
                            )),
                        ),
                    );
                }
            }
        });
    }
}

#[tokio::main]
async fn main() {
    let limiters: NodeHashMap = Arc::new(Mutex::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel::<(IpAddr, [u8; 16], [u8; 8], oneshot::Sender<bool>)>(4096);

    let vm_thread = thread::spawn(move || {
        let cache = RandomXCache::new(RandomXFlag::FLAG_DEFAULT, pow::SEED_HASH).unwrap();
        let dataset = RandomXDataset::new(RandomXFlag::FLAG_DEFAULT, cache, 0).unwrap();
        let vm = RandomXVM::new(
            RandomXFlag::FLAG_HARD_AES | RandomXFlag::FLAG_FULL_MEM | RandomXFlag::FLAG_JIT,
            None,
            Some(dataset),
        )
        .unwrap();

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
