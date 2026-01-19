//! The client only tries to solve the RandomX challenge.
use randomx_rs::{RandomXCache, RandomXDataset, RandomXFlag, RandomXVM};
use throttling::pow;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const CONTROL_ADDR: &str = "127.0.0.1:9978";

#[tokio::main]
async fn main() {
    let mut stream = TcpStream::connect(CONTROL_ADDR).await.unwrap();
    let mut solution = getrandom::u64().unwrap();

    let cache = RandomXCache::new(RandomXFlag::FLAG_DEFAULT, pow::SEED_HASH).unwrap();

    let now = std::time::Instant::now();
    println!("initializing dataset (only needs to be done once, at node startup)...");
    let dataset = RandomXDataset::new(RandomXFlag::FLAG_DEFAULT, cache, 0).unwrap();
    println!("initialized, took {:.2?}\n", now.elapsed());

    let vm = RandomXVM::new(
        RandomXFlag::FLAG_HARD_AES | RandomXFlag::FLAG_FULL_MEM | RandomXFlag::FLAG_JIT,
        None,
        Some(dataset),
    )
    .unwrap();

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
