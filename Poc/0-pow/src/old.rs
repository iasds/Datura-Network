use std::env;

use std::net::TcpListener;
use std::net::TcpStream;
use std::io::{self, Read, Write};

fn connect(bootstrap_node:&str) -> io::Result<TcpStream> {
    let stream = TcpStream::connect(&bootstrap_node)?;
    Ok(stream)
}

fn listen(port: &str) -> () {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).unwrap();
    println!("started node on {}", port);

    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        let mut buffer = [0; 1024];

        let bytes_read = stream.read(&mut buffer);
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    println!("hello");
    if args.len() < 2 || args.len() > 3 {
        println!("args error");
        return
    }

    if args.len() == 2{
        listen(&args[1]);
    }

    if args.len() == 3{
        println!("hi");
        match connect(&args[2]) {
            Ok(mut stream) => {
                println!("Successfully connected!");
                
                // Send data
                stream.write_all(b"Hello, server!")?;
                
                // Read response
                let mut buffer = [0; 512];
                let bytes_read = stream.read(&mut buffer)?;
                println!("Received: {}", String::from_utf8_lossy(&buffer[..bytes_read]));
                
                Ok(())
            }
            Err(e) => {
                eprintln!("Failed to connect: {}", e);
                Err(e)
            }
        }
        listen(&args[1]);
    }
    
}