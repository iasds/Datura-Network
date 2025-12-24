use std::io::Read;

fn main() {
	let args: Vec<String> = std::env::args().collect();

	let mut port = 9051;

	if args.len() > 1 { port = args[1].parse().unwrap(); }

	// spawn UDP listener
	std::thread::spawn(move || {
    	let udp_socket = std::net::UdpSocket::bind(format!("127.0.0.1:{}", port)).unwrap();

		let mut buf = [0; 1024];
		loop {
			let (amt, src) = udp_socket.recv_from(&mut buf).unwrap();

			println!("UDP: {} Bytes from {:?}, '{:02X?}'", amt, src, &buf[..amt]);
		}
	});

	let tcp_listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).unwrap();

	println!("Succeeded in binding UDP and TCP on port {}", port);

	// loop to spawn TCP listener for each connection
    for stream in tcp_listener.incoming() {
		let mut stream = stream.unwrap();

		std::thread::spawn(move || {
			let mut buf = [0; 1024];
			loop {
				match stream.read(&mut buf) {
					Ok(0) => break,
					Err(_) => break,
					Ok(amt) => { 
						println!("TCP: {} Bytes from {:?}, '{:02X?}'", amt, stream.peer_addr().unwrap(), &buf[..amt]);
					}
				}
			}
		});
    }
}
