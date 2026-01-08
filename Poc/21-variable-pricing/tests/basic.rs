use tokio;

#[test]
pub fn basic_test() {
    tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap()
    .block_on(async_tests());
}

pub async fn async_tests() {
    println!("Hello world");
}
