use std::io::Read;
use std::io::Write;
use std::str::from_utf8;
use std::thread;

const CHALLENGE_DIFFICULTY: u32 = 4000;

mod equix_pow;
use crate::equix_pow::*;

fn main() {
    // parse port args
    let args: Vec<String> = std::env::args().collect();
    let mut port = 9051;
    if args.len() > 2 {
        port = args[2].parse().unwrap();
    } else if args.len() > 1 {
        port = args[1].parse().unwrap();
    }

    if args.len() > 2 {
        // run as client

        match std::net::TcpStream::connect(format!("127.0.0.1:{}", port)) {
            Err(e) => {
                println!("failed to connect to server at 127.0.0.1:{}: {}", port, e);
            }

            Ok(mut stream) => {
                println!("successfully connected to server at 127.0.0.1:{}", port);

                let mut challenge_buf = [0u8; 16];

                // read challenge
                match stream.read_exact(&mut challenge_buf) {
                    Err(e) => {
                        println!("failed to read challenge from server: {}", e);
                    }

                    Ok(_) => {
                        let challenge = u128::from_le_bytes(challenge_buf);
                        println!(
                            "got challenge '{:X}' of difficulty {} from server",
                            u128::from_le_bytes(challenge_buf),
                            get_challenge_effort(challenge)
                        );

                        // solve challenge or generate random solution
                        let mut solution = [0u8; 24];
                        if args[1].parse::<String>().unwrap() == "random" {
                            println!("generating random solution...");
                            getrandom::fill(&mut solution).unwrap();
                        } else {
                            println!("solving challenge...");
                            solution = solve_challenge(
                                thread::available_parallelism().unwrap().get(),
                                challenge,
                            );
                            println!("solved challenge");
                        }

                        println!("sent challenge to server");
                        let _ = stream.write(&solution);

                        // read and print node list from server
                        let mut node_list_buf = [0u8; 1024];
                        match stream.read(&mut node_list_buf) {
                            Err(e) => {
                                println!("failed to read node list from server: {}", e);
                            }

                            Ok(amt) => {
                                println!(
                                    "server replied with {} bytes \n\"{}\"",
                                    amt,
                                    from_utf8(&node_list_buf[..amt]).unwrap()
                                );
                            }
                        }
                    }
                }
            }
        }
    } else {
        // run as server

        let hardcoded_node_list = "127.0.0.1:10000
127.0.0.1:10001
127.0.0.1:10002
127.0.0.1:20000
127.0.0.1:20001";

        let tcp_listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).unwrap();

        // tcp listen loop
        for stream in tcp_listener.incoming() {
            let mut stream = stream.unwrap();

            std::thread::spawn(move || {
                let peer_addr = stream.peer_addr().unwrap();

                // create and send challenge
                let challenge = create_challenge(CHALLENGE_DIFFICULTY);
                let _ = stream.write(&challenge.to_le_bytes());
                println!(
                    "{} connected, sent difficulty {} challenge '{:X}'",
                    peer_addr, CHALLENGE_DIFFICULTY, challenge
                );

                // read solution and send node list if correct
                let mut solution_buf = [0u8; 24];
                match stream.read_exact(&mut solution_buf) {
                    Err(_) => {
                        println!("failed to read solution from {}", peer_addr);
                    }
                    Ok(_) => {
                        println!("{} replied with solution", peer_addr);

                        // verify solution and send node list if correct
                        if verify_solution(CHALLENGE_DIFFICULTY, challenge, solution_buf) {
                            println!(
                                "solution from {} was correct, sending list of nodes",
                                peer_addr
                            );

                            stream.write_all(hardcoded_node_list.as_bytes()).unwrap();
                        } else {
                            println!("solution from {} was incorrect, closing stream", peer_addr);
                        }

                        println!();
                    }
                }
            });
        }
    }
}
