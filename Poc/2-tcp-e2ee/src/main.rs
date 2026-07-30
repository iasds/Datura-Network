use std::io::Read;
use std::io::Write;

use hpke_rs::{hpke_types::*, *};
use hpke_rs_libcrux::HpkeLibcrux;

fn main() {
    // parse port args
    let args: Vec<String> = std::env::args().collect();
    let mut port = 10000;
    if args.len() > 2 {
        port = args[2].parse().unwrap();
    } else if args.len() > 1 {
        port = args[1].parse().unwrap();
    }

    // set up encryption options
    let mut hpke = Hpke::<HpkeLibcrux>::new(
        Mode::Base,
        KemAlgorithm::XWingDraft06,
        KdfAlgorithm::HkdfSha256,
        AeadAlgorithm::ChaCha20Poly1305,
    );

    if args.len() > 2 {
        // run as client

        match std::net::TcpStream::connect(format!("127.0.0.1:{}", port)) {
            Err(e) => {
                println!("failed to connect to server at 127.0.0.1:{}: {}", port, e);
            }

            Ok(mut stream) => {
                println!("successfully connected to server at 127.0.0.1:{}", port);

                let mut server_public_key = [0u8; 1216];

                match stream.read_exact(&mut server_public_key) {
                    Err(e) => {
                        println!("failed to read key from server: {}", e);
                    }

                    Ok(_) => {
                        let key = HpkePublicKey::new(server_public_key.to_vec());

                        // derive shared key from server public key using X-Wing
                        let now = std::time::Instant::now();
                        let (ciphertext, mut sender_context) =
                            hpke.setup_sender(&key, b"", None, None, None).unwrap();
                        println!("took {:?} for client KEM", now.elapsed());

                        let _ = stream.write(&ciphertext);
                        println!("sent KEM ciphertext to server");

                        let plaintext = vec![1u8; 100000000];

                        // encrypt data with ChaCha20 to send to server
                        let now = std::time::Instant::now();
                        let encrypted = sender_context.seal(b"", &plaintext).unwrap();
                        println!("took {:?} to encrypt 100mb", now.elapsed());

                        let _ = stream.write(&encrypted);
                        println!("sent encrypted data to server");
                    }
                }
            }
        }
    } else {
        // run as server

        let (secret_key, public_key) = hpke.generate_key_pair().unwrap().into_keys();

        let tcp_listener = std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).unwrap();

        // tcp listen loop
        for stream in tcp_listener.incoming() {
            let mut stream = stream.unwrap();

            let public_key_local = public_key.clone();

            let peer_addr = stream.peer_addr().unwrap();

            let _ = stream.write(public_key_local.as_slice());
            println!("{} connected", peer_addr);

            let mut ciphertext = [0u8; 1120];

            // read ciphertext
            match stream.read_exact(&mut ciphertext) {
                Err(_) => {
                    println!("failed to read ciphertext from {}", peer_addr);
                    return;
                }
                Ok(_) => {
                    println!(
                        "received KEM ciphertext from {}, deriving shared key",
                        peer_addr
                    );

                    // derive shared key from client ciphertext using X-Wing
                    let now = std::time::Instant::now();
                    let mut receiver_context = hpke
                        .setup_receiver(&ciphertext, &secret_key, b"", None, None, None)
                        .unwrap();
                    println!("took {:?} for server KEM", now.elapsed());

                    let mut encrypted = vec![0u8; 100000016];

                    // read encrypted data
                    match stream.read_exact(&mut encrypted) {
                        Err(_) => {
                            println!("failed to read encrypted data from {}", peer_addr);
                            return;
                        }

                        Ok(_) => {
                            println!("finished reading encrypted data");

                            // decrypt data using ChaCha20
                            let now = std::time::Instant::now();
                            let plaintext = receiver_context.open(b"", &encrypted).unwrap();
                            println!("took {:?} to decrypt 100mb", now.elapsed());

                            // check that decrypted data is correct
                            for num in plaintext {
                                assert!(num == 1);
                            }

                            println!("data received was correct");
                        }
                    }
                }
            }
        }
    }
}
