
#[tokio::main]
async fn main() {
    xmr_pow_challenges::Client::new(Some("127.0.0.1:3355".to_string())).await.unwrap();
}
