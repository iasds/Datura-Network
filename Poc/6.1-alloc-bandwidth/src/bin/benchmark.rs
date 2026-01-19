use rand::Rng;
use std::future::Future;
use std::hint::black_box;
use std::time::Instant;

const ITERS: usize = 3_000_000;

async fn bench_getrandom() -> u128 {
    let mut buf = [0u8; 16];

    let start = Instant::now();

    for _ in 0..ITERS {
        getrandom::fill(&mut buf).unwrap();
        black_box(&buf);
    }

    start.elapsed().as_nanos()
}

async fn bench_rand() -> u128 {
    let mut buf = [0u8; 16];
    let mut rng = rand::rng();

    let start = Instant::now();
    for _ in 0..ITERS {
        rng.fill(&mut buf);
        black_box(&buf);
    }

    start.elapsed().as_nanos()
}

// Run both benchmarks separately for each thread count
async fn run_bench<F>(name: &str, bench_fn: fn() -> F)
where
    F: Future<Output = u128> + Send + 'static,
{
    let total_ns: u128 = tokio::spawn(bench_fn()).await.unwrap();
    let ns_per_call = total_ns as f64 / ITERS as f64;

    println!(
        "{:<20} | {:>8.2} ns/call | {:.2} Mops/s",
        name,
        ns_per_call,
        1e3 / ns_per_call
    );
}

#[tokio::main]
async fn main() {
    run_bench("getrandom::fill", bench_getrandom).await;
    run_bench("rand::rng", bench_rand).await;
}
