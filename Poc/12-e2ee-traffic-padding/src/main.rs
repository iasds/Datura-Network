use std::{
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Write},
    net::{TcpListener, TcpStream}
};

use hpke_rs::{*, hpke_types::*};
use hpke_rs_libcrux::HpkeLibcrux;


// Can be changed
const PACKET_SIZE: usize = 1337;
// tag for poly1305
const AEAD_TAG_SIZE: usize = 16;
// Indicates length of padding; uses 2 bytes
const PADDING_INDICATOR_SIZE: usize = 2;

const KEM_PUBKEY_LEN: usize = 1216;
const CIPHERTEXT_LEN: usize = 1120;

const NUM_PACKETS_KEM_PUBKEY: usize = (CIPHERTEXT_LEN as f32 / PACKET_SIZE as f32).ceil() as usize;
const NUM_PACKETS_KEM_CIPHERTEXT: usize = (CIPHERTEXT_LEN as f32 / PACKET_SIZE as f32).ceil() as usize;

const KEM_PUBKEY_PADDING_AMOUNT: usize = NUM_PACKETS_KEM_PUBKEY * PACKET_SIZE - KEM_PUBKEY_LEN;
const KEM_C_PADDING_AMOUNT: usize = NUM_PACKETS_KEM_CIPHERTEXT * PACKET_SIZE - CIPHERTEXT_LEN;



fn usage_and_die() {
    eprintln!("Usage:");
    eprintln!();
    eprintln!("cargo run -- server [server-listen-port]");
    eprintln!("cargo run -- client [client-listen-port] [server-listen-port]");
    std::process::exit(-1);
}


fn calc_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}


fn send_equal_data_per_packet(stream: &mut TcpStream, data: &Vec<u8>) -> usize{
    /*
     * Returns amount of data sent.
     */

    // data.len() should be a multiple of PACKET_SIZE already
    let num_packets = data.len() / PACKET_SIZE;
    let mut start = 0;
    for _ in 0..num_packets {
        stream.write(&data[start..(start + PACKET_SIZE)]).unwrap();
        start += PACKET_SIZE;
    }
    start
}


fn receive_packets(stream: &mut TcpStream, out_buf: &mut Vec<u8>) -> usize {
    /*
     * Returns size of last packet read.
     */
    loop {
        let mut buf: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
        let s = stream.read(&mut buf).unwrap();

        if s == 0 {
            return 0;
        }

        // Extend after checking s == 0, so if 0 bytes are read, then that doesn't
        // get added to vector
        out_buf.extend(&buf);

        if s < PACKET_SIZE {
            return s;
        }

    }
}


fn main() {

    println!("PACKET_SIZE is {PACKET_SIZE}");
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        usage_and_die();
    }

    let role = &args[1]; // server (node B) or client (node A)
    if role.to_lowercase() != "server" && role.to_lowercase() != "client" {
        usage_and_die();
    }

    let mut hpke = Hpke::<HpkeLibcrux>::new(
        Mode::Base,
        KemAlgorithm::XWingDraft06,
        KdfAlgorithm::HkdfSha256,
        AeadAlgorithm::ChaCha20Poly1305
    );



    if role == "server" { // Node B


        let server_listen_port: i32 = args[2].parse().unwrap();
        let listener = TcpListener::bind(format!("127.0.0.1:{}", server_listen_port)).unwrap();
        println!("[Node B] Listening on {server_listen_port}");

        for stream in listener.incoming() {
            let mut stream = stream.unwrap();

            let now = std::time::Instant::now();

            let (server_sk, server_pk) = hpke.generate_key_pair().unwrap().into_keys();
            let mut padded_pk: Vec<u8> = server_pk.as_slice().to_vec();
            // Use 0s for padding here, but should implement random bytes to hide
            // the fact that this is a key.
            // 1216 is X-Wing ecapsulation public key length.
            padded_pk.extend_from_slice(&[0 as u8; KEM_PUBKEY_PADDING_AMOUNT]);
            send_equal_data_per_packet(&mut stream, &padded_pk);
            println!("[Node B] Sent encapsulation key to Node A in {:?}", now.elapsed());


            // Receive padded ciphertext.

            let mut padded_c: Vec<u8> = Vec::new();
            for _ in 0..NUM_PACKETS_KEM_CIPHERTEXT {
                let mut buf: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
                stream.read_exact(&mut buf).unwrap();
                padded_c.extend(buf);
            }

            println!("[Node B] Received ciphertext from Node A");

            // X-Wing ciphertext len is 1120 bytes, so chop of extra padding
            let c = &padded_c[..CIPHERTEXT_LEN];

            let now = std::time::Instant::now();
            let mut receiver_context = hpke.setup_receiver(
                &c, &server_sk, b"", None, None, None).unwrap();
            println!("[Node B] Derived shared secret in {:?}", now.elapsed());


            // Process encrypted data

            let mut encrypted: Vec<u8> = vec![];

            receive_packets(&mut stream, &mut encrypted);

            println!("[Node B] Encrypted size: {} bytes", encrypted.len());
            println!("[Node B] Encrypted hash: {}", calc_hash(&encrypted));


            let now = std::time::Instant::now();
            let decrypted: Vec<u8> = receiver_context.open(b"", &encrypted).unwrap();
            println!("[Node B] Decrypted data in {:?}", now.elapsed());
            let decrypted_len = decrypted.len();

            // top + bottom half of number that indicates padding amount
            let top_half_len_padding = decrypted[decrypted_len - 2] as i32;
            let bottom_half_len_padding = decrypted[decrypted_len - 1] as i32;

            let len_padding: i32 = (top_half_len_padding << 8 & 0xff00) | (bottom_half_len_padding & 0xff);


            println!("[Node B] Decrypted size: {} bytes", decrypted_len);
            println!("[Node B] Decrypted hash: {}", calc_hash(&decrypted));
            println!("[Node B] Len padding: {len_padding}");
            println!("\n");
        }


    } else { // client Node A
        let client_listen_port: i32 = args[2].parse().unwrap();
        let server_listen_port: i32 = args[3].parse().unwrap();

        println!("[Node A] Listening on {client_listen_port}");
        println!("[Node A] Will connect to Node B on {server_listen_port}");

        // Node A listens for traffic to send to Node B
        let listener = TcpListener::bind(format!("127.0.0.1:{}", client_listen_port)).unwrap();


        for stream in listener.incoming() {

            let mut recv_stream = stream.unwrap();

            // Connection bewteen Node A and Node B
            let mut send_stream = TcpStream::connect(format!("127.0.0.1:{}", server_listen_port)).unwrap();


            // Receive KEM pubkey and setup ciphertext
            let mut padded_pubkey: Vec<u8> = Vec::new();
            for _ in 0..NUM_PACKETS_KEM_PUBKEY {
                let mut buf: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
                send_stream.read_exact(&mut buf).unwrap();
                padded_pubkey.extend(buf);
            }

            println!("[Node A] Received encapsulation key from Node B");

            let now = std::time::Instant::now();

            let server_pubkey = HpkePublicKey::new(padded_pubkey[..KEM_PUBKEY_LEN].to_vec());
            let (c, mut sender_context) =
                hpke.setup_sender(&server_pubkey, b"", None, None, None).unwrap();

            let mut padded_c: Vec<u8> = Vec::new();
            padded_c.extend_from_slice(&c);
            padded_c.extend_from_slice(&[0 as u8; KEM_C_PADDING_AMOUNT]);

            // Send ciphertext to server
            send_equal_data_per_packet(&mut send_stream, &padded_c);
            println!("[Node A] Created and sent KEM ciphertext to Node B in {:?}", now.elapsed());


            let mut msg = Vec::new();

            let now = std::time::Instant::now();

            // informs for length of padding. Essentially this is the same
            // as len(total read data) % (PACKET_SIZE); len of the last chunk
            let last_chunk_size = receive_packets(&mut recv_stream, &mut msg);

            println!("[Node A] Received data to send in {:?}", now.elapsed());



            // Encrypted packet format:
            // data
            // padding (0- (PACKET_SIZE - 16 - 2) bytes)
            // len_padding (2 bytes)
            // AEAD tag (16 bytes)


            let padding_len: usize;
            // Need 2 bytes for len of padding, 16 for chacha20poly1305 tag.
            // If < 18 bytes left at end of last chunk, need a new chunk.
            if (PACKET_SIZE - last_chunk_size) < (PADDING_INDICATOR_SIZE + AEAD_TAG_SIZE) {

                // Add padding for original chunk (going to be < 18 bytes)
                let len_padding_on_first_chunk = PACKET_SIZE - last_chunk_size;
                // Need padding for another chunk that can fit both len_padding and aead tag
                let len_padding_new_chunk = PACKET_SIZE - (PADDING_INDICATOR_SIZE + AEAD_TAG_SIZE);
                padding_len = len_padding_on_first_chunk + len_padding_new_chunk;
                // Fill the remaining <18 bytes with 0, then add 0s the size of another packet.
                // The last 16 will be removed to make space for the aead tag.
                msg.extend(vec![0; last_chunk_size]);
                msg.extend([0 as u8; PACKET_SIZE]);

            } else {
                // This case, there is room for 2 + 16 bytes at end. No need
                // to do anything else here
                padding_len = (PACKET_SIZE - AEAD_TAG_SIZE) - last_chunk_size;
            }

            let msg_len = msg.len();

            // Remove AEAD_TAG_SIZE bytes off the end (guaranteed last 18 bytes
            // are 0 at this point, because msg vector was extended with [0; PACKET_SIZE]
            // in loop and in check above; and we checked there was enough room)
            for i in 1..=AEAD_TAG_SIZE {
                msg.remove(msg_len - i);
            }

            // IMPORTANT: recalculate len so the length indicator bytes
            // can be put at the end
            let msg_len = msg.len();


            let top_half_len_padding = (padding_len >> 8 & 0xff) as u8;
            let bottom_half_len_padding = (padding_len >> 0 & 0xff) as u8;

            msg[msg_len - 2] = top_half_len_padding;
            msg[msg_len - 1] = bottom_half_len_padding;


            let now = std::time::Instant::now();
            // No AAD
            let msg_encrypted: Vec<u8> = sender_context.seal(
                b"", &msg).unwrap();
            println!("[Node A] Encrypted padded data in {:?}", now.elapsed());

            println!("[Node A] Unencrypted size: {msg_len} bytes");
            println!("[Node A] Unencrypted hash: {}", calc_hash(&msg));
            println!("[Node A] Encrypted size: {} bytes", msg_encrypted.len());
            println!("[Node A] Encrypted hash: {}", calc_hash(&msg_encrypted));
            println!("[Node A] Number of padding bytes: {padding_len}");


            let num_packets = msg_encrypted.len() / PACKET_SIZE;

            let now = std::time::Instant::now();
            println!("[Node A] Sending {num_packets} packets");

            let bytes_sent = send_equal_data_per_packet(&mut send_stream, &msg_encrypted);

            println!("[Node A] Sent {} bytes in {:?}", bytes_sent, now.elapsed());
            println!("\n");

        }

    }

}
