use tokio::net::TcpStream;

const DATA_PATH: &str = "/tmp/datura/store";
const BUFFER_SIZE: usize = 8192;

// inspired from https://github.com/tokio-rs/tokio/blob/master/examples/echo-tcp.rs
pub async fn read_from(mut socket: TcpStream, data_len: usize) {
	let mut buf = vec![0; BUFFER_SIZE];
	todo!();
}
