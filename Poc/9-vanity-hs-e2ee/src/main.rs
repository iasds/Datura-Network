mod address;
mod crypto;

use address::{address_to_pubkey, pubkey_to_address};
use crypto::{KEM_OUTPUT_LEN, decrypt, encrypt};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use std::{
    env,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};

// Maximum accepted ciphertext size (1 Mib). Prevents a peer from triggering a
// ~4 Gib allocation by sending 0xFFFFFFFF as the 4-byte length prefix.
const MAX_CIPHERTEXT_LEN: usize = 1024 * 1024;

// Read timeout applied to every accepted connection.
// A client that connects but never sends will block the single-threaded server
// forever without this guard.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("server") => {
            let port = args.get(2).map(String::as_str).unwrap_or("9009");
            run_server(port);
        }
        Some("client") => match (args.get(2), args.get(3)) {
            (Some(address), Some(port)) => {
                let msg = args
                    .get(4)
                    .map(String::as_str)
                    .unwrap_or("hello datura network!");
                run_client(address, port, msg.as_bytes());
            }
            _ => {
                eprintln!("usage: vanity-hs-e2ee client <address.dn> <port> [message]");
                std::process::exit(1);
            }
        },
        _ => {
            eprintln!("usage:");
            eprintln!(
                "  vanity-hs-e2ee server [port]                          # default port: 9009"
            );
            eprintln!(
                "  vanity-hs-e2ee client <address.dn> <port> [message]   # default: hello datura network!"
            );
            std::process::exit(1);
        }
    }
}

fn run_server(port: &str) {
    if port.parse::<u16>().is_err() {
        eprintln!("invalid port: `{port}` (must be 1-65535)");
        std::process::exit(1);
    }

    let signing_key = SigningKey::generate(&mut OsRng);
    let address = pubkey_to_address(&signing_key.verifying_key());

    println!("hidden service address: {address}");

    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap_or_else(|e| {
        eprintln!("failed to bind 127.0.0.1:{port}: {e}");
        std::process::exit(1);
    });

    println!("listening on port {port}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut client_stream) => handle_connection(&mut client_stream, &signing_key),
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
}

fn handle_connection(stream: &mut TcpStream, signing_key: &SigningKey) {
    let peer = stream
        .peer_addr()
        .map(|socket_addr| socket_addr.to_string())
        .unwrap_or_else(|_| "unknown".into());

    println!("[{peer}] connected");

    // Guard against a client that connects but never sends
    // without a timeout the single-threaded server would block here forever
    if let Err(e) = stream.set_read_timeout(Some(READ_TIMEOUT)) {
        eprintln!("[{peer}] failed to set read timeout: {e}");
        return;
    }

    // Wire format (sent by client):
    //   [kem_output: 32 bytes fixed]
    //   [ciphertext_len: 4 bytes big-endian]
    //   [ciphertext: ciphertext_len bytes]

    let mut kem_output = [0u8; KEM_OUTPUT_LEN];
    if let Err(e) = stream.read_exact(&mut kem_output) {
        eprintln!("[{peer}] failed to read kem output: {e}");
        return;
    }

    let mut len_buf = [0u8; 4];
    if let Err(e) = stream.read_exact(&mut len_buf) {
        eprintln!("[{peer}] failed to read ciphertext length: {e}");
        return;
    }
    let ct_len = u32::from_be_bytes(len_buf) as usize;

    // ChaCha20Poly1305 always appends a 16-byte tag; anything shorter is invalid.
    const AEAD_TAG: usize = 16;
    if ct_len < AEAD_TAG {
        eprintln!("[{peer}] ciphertext length {ct_len} too short (minimum {AEAD_TAG}), closing");
        return;
    }
    // Reject absurdly large lengths before allocating, a peer can set this to
    // 0xFFFFFFFF (~4 Gib) which would OOM-kill the process.
    if ct_len > MAX_CIPHERTEXT_LEN {
        eprintln!(
            "[{peer}] ciphertext length {ct_len} exceeds limit {MAX_CIPHERTEXT_LEN}, closing"
        );
        return;
    }

    let mut ciphertext = vec![0u8; ct_len];
    if let Err(e) = stream.read_exact(&mut ciphertext) {
        eprintln!("[{peer}] failed to read ciphertext: {e}");
        return;
    }

    match decrypt(signing_key, &kem_output, &ciphertext) {
        Ok(plaintext) => println!(
            "[{peer}] decrypted: {:?}",
            String::from_utf8_lossy(&plaintext)
        ),
        Err(e) => eprintln!("[{peer}] decryption failed: {e:?}"),
    }
}

fn run_client(address: &str, port: &str, message: &[u8]) {
    if port.parse::<u16>().is_err() {
        eprintln!("invalid port: `{port}` (must be 1-65535)");
        std::process::exit(1);
    }

    // The entire key exchange is derived from the address alone.
    let recipient_verifying_key = address_to_pubkey(address).unwrap_or_else(|e| {
        eprintln!("invalid address: {e}");
        std::process::exit(1);
    });

    println!("extracted pubkey from address; no network lookup needed");

    let (kem_output, ciphertext) = encrypt(&recipient_verifying_key, message).unwrap_or_else(|e| {
        eprintln!("encryption failed: {e:?}");
        std::process::exit(1);
    });

    println!("message: {:?}", String::from_utf8_lossy(message));
    println!(
        "encrypted {} bytes  -->  kem: {} bytes, ciphertext: {} bytes",
        message.len(),
        kem_output.len(),
        ciphertext.len(),
    );

    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap_or_else(|e| {
        eprintln!("failed to connect to 127.0.0.1:{port}: {e}");
        std::process::exit(1);
    });

    // Send kem_output (fixed 32 bytes), then length-prefixed ciphertext.
    if let Err(e) = stream.write_all(&kem_output) {
        eprintln!("failed to send kem output: {e}");
        std::process::exit(1);
    }
    if let Err(e) = stream.write_all(&(ciphertext.len() as u32).to_be_bytes()) {
        eprintln!("failed to send ciphertext length: {e}");
        std::process::exit(1);
    }
    if let Err(e) = stream.write_all(&ciphertext) {
        eprintln!("failed to send ciphertext: {e}");
        std::process::exit(1);
    }

    println!("sent encrypted message to server");
}
