use std::env;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

type NodeList = Arc<Mutex<Vec<String>>>;

// fn get_public_ip() -> io::Result<String> {
//     let mut stream = TcpStream::connect("api.ipify.org:80")?;
//     let mut response = String::new();
    
//     stream.write_all(b"GET / HTTP/1.0\r\nHost: api.ipify.org\r\n\r\n")?;
//     stream.read_to_string(&mut response)?;

//     let ip = response
//         .split("\r\n\r\n")
//         .last()
//         .ok_or(io::Error::new(io::ErrorKind::Other, "Couldn't generate local public ip"))?
//         .trim()
//         .to_string();

//     Ok(ip)
// }

fn handle_incoming(mut stream: TcpStream, nodes: NodeList) -> io::Result<()> {
    let mut port_buffer = [0; 64];
    let bytes_read = stream.read(&mut port_buffer)?;

    let connecting_node_ip = stream.peer_addr()?.ip().to_string();
    let connecting_node_port = String::from_utf8_lossy(&port_buffer[..bytes_read]).trim().to_string();

    let connecting_node_addr = format!("{}:{}", connecting_node_ip, connecting_node_port);

    println!("New node connected: {}", connecting_node_addr);

    let mut nodes = nodes.lock().unwrap();

    if !nodes.contains(&connecting_node_addr) {
        nodes.push(connecting_node_addr.clone());
    }

    let nodes_list_str = nodes.join("\n");
    stream.write_all(nodes_list_str.as_bytes())?;

    Ok(())
}

fn connect(bootstrap_addr: &str, my_port: &str, nodes: NodeList) -> io::Result<()> {
    let mut stream = TcpStream::connect(bootstrap_addr)?;

    println!("Connected to node {}", bootstrap_addr);

    stream.write_all(my_port.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    let mut nodes = nodes.lock().unwrap();
    
    for line in response.lines() {
        let addr = line.trim().to_string();

        if !nodes.contains(&addr) {
            nodes.push(addr.clone());
        }
    }

    Ok(())
}

fn listen(port: &str, nodes: NodeList) -> io::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;

    println!("Listening on port {}", port);

    for stream in listener.incoming() {
        let stream = stream?;
        let nodes = Arc::clone(&nodes);
        handle_incoming(stream, nodes)?;
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let nodes: NodeList = Arc::new(Mutex::new(Vec::new()));
    
    if args.len() < 2 {
        eprintln!("Usage: {} <port> [bootstrap_addr]", args[0]);
        std::process::exit(1);
    }

    // let my_ip = get_public_ip()?;
    // let my_addr = format!("{}:{}", my_ip, &args[1]);
    // nodes.lock().unwrap().push(my_addr);

    match args.len() {
        2 => listen(&args[1], nodes),
        3 => {
            connect(&args[2], &args[1], Arc::clone(&nodes))?;
            return listen(&args[1], nodes);
        }
        _ => {
            eprintln!("Usage: {} <port> [bootstrap_addr]", args[0]);
            std::process::exit(1);
        }
    }?;

    Ok(())
}