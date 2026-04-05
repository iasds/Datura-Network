use std::{
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Write},
    net::{TcpListener, TcpStream}
};

use hpke_rs::{*, hpke_types::*};
use hpke_rs_libcrux::HpkeLibcrux;


// Can be changed
const PACKET_SIZE: usize = 1024;
// tag for poly1305
const AEAD_TAG_SIZE: usize = 16;
// Indicates length of padding; uses 2 bytes
const PADDING_INDICATOR_SIZE: usize = 2;

const PUB_ENCAP_KEY_LEN: usize = 1216;
const CIPHERTEXT_LEN: usize = 1120;

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

fn main() {

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


    if role == "server" { // B

        let (sk, pk) = hpke.generate_key_pair().unwrap().into_keys();

        let server_listen_port: i32 = args[2].parse().unwrap();
        let listener = TcpListener::bind(format!("127.0.0.1:{}", server_listen_port)).unwrap();
        println!("[Node B] Listening on {server_listen_port}");

        for stream in listener.incoming() {
            let mut stream = stream.unwrap();

            let mut padded_pk: Vec<u8> = pk.as_slice().to_vec();
            // Use 0s for padding here, but should implement random bytes to hide
            // the fact that this is a key.
            // 1216 is X-Wing ecapsulation public key length. Need two packets
            // to fit one pubkey, hence 2 * PACKET_SIZE. Subtract len, since that
            // space is taken up by pubkey already.
            padded_pk.extend_from_slice(&[0 as u8; 2 * PACKET_SIZE - PUB_ENCAP_KEY_LEN]);
            // send first half of PK to client
            stream.write(&padded_pk[..PACKET_SIZE]).unwrap();
            // send second half
            stream.write(&padded_pk[PACKET_SIZE..]).unwrap();

            println!("[Node B] Sent encapsulation key to Node A");
            // Receive padded ciphertext. It is two packets wide.
            let mut padded_c = [0 as u8; PACKET_SIZE * 2];
            stream.read_exact(&mut padded_c).unwrap();
            println!("[Node B] Received ciphertext from Node A");

            // X-Wing ciphertext len is 1120 bytes, so chop of extra padding
            let c = &padded_c[..CIPHERTEXT_LEN];

            let mut receiver_context = hpke.setup_receiver(
                &c, &sk, b"", None, None, None).unwrap();


            let mut encrypted: Vec<u8> = vec![];

            loop {
                let mut buf: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
                let s = stream.read(&mut buf).unwrap();
                if s == 0 {
                    break;
                }
                // Extend after checking s, so if 0 bytes are read, then that doesn't
                // get added to vector
                encrypted.extend(&buf);
            }

            //println!("[Node B] {:?}", encrypted);

            println!("[Node B] Encrypted size: {} bytes", encrypted.len());
            println!("[Node B] Encrypted hash: {}", calc_hash(&encrypted));


            let decrypted: Vec<u8> = receiver_context.open(b"", &encrypted).unwrap();
            let decrypted_len = decrypted.len();

            // top + bottom half of number that indicates padding amount
            let top_half_len_padding = decrypted[decrypted_len - 2] as i32;
            let bottom_half_len_padding = decrypted[decrypted_len - 1] as i32;

            let len_padding: i32 = (top_half_len_padding << 8 & 0xff00) | (bottom_half_len_padding & 0xff);


            println!("[Node B] Decrypted size: {} bytes", decrypted_len);
            println!("[Node B] Decrypted hash: {}", calc_hash(&decrypted));
            println!("[Node B] Len padding: {len_padding}");
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

            // These aren't used
            //let (sk, pk) = hpke.generate_key_pair().unwrap().into_keys();

            // Connection bewteen Node A and Node B
            let mut send_stream = TcpStream::connect(format!("127.0.0.1:{}", server_listen_port)).unwrap();

            let mut first_half_padded_pk = [0u8; PACKET_SIZE];
            let mut second_half_padded_pk = [0u8; PACKET_SIZE];
            send_stream.read_exact(&mut first_half_padded_pk).unwrap();
            send_stream.read_exact(&mut second_half_padded_pk).unwrap();
            let mut server_padded_pk = Vec::new();
            server_padded_pk.extend_from_slice(&first_half_padded_pk);
            server_padded_pk.extend_from_slice(&second_half_padded_pk);

            println!("[Node A] Received encapsulation key from Node B");

            let server_pk_buf = &server_padded_pk[..PUB_ENCAP_KEY_LEN];

            let server_pk = HpkePublicKey::new(server_pk_buf.to_vec());
            let (c, mut sender_context) =
                hpke.setup_sender(&server_pk, b"", None, None, None).unwrap();


            let mut padded_c: Vec<u8> = Vec::new();
            padded_c.extend_from_slice(&c);
            padded_c.extend_from_slice(&[0 as u8; 2 * PACKET_SIZE - CIPHERTEXT_LEN]);

            // Send ciphertext to server
            send_stream.write(&padded_c[..PACKET_SIZE]).unwrap();
            send_stream.write(&padded_c[PACKET_SIZE..]).unwrap();
            println!("[Node A] Sent ciphertext to Node B");


            let mut msg = Vec::new();

            // informs for length of padding
            let last_chunk_size;
            // read data to be sent to server node b
            loop {
                let mut buf: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
                let s = recv_stream.read(&mut buf).unwrap();
                msg.extend(&buf);
                if s < PACKET_SIZE {
                    last_chunk_size = s;
                    break;
                }
            }



            // Encrypted packet format:
            // data
            // padding (0- (PACKET_SIZE - 16 - 2) bytes)
            // len_padding (2 bytes)
            // AEAD tag (16 bytes)

            let padding_len: usize;
            // Need 2 bytes for len of padding, 16 for chacha20poly1305 tag.
            // If < 2 bytes left at end of last chunk, need a new chunk.
            if (PACKET_SIZE - last_chunk_size) < (PADDING_INDICATOR_SIZE + AEAD_TAG_SIZE) {

                // Add padding for original chunk
                let len_padding_on_first_chunk = PACKET_SIZE - last_chunk_size;
                // Need padding for another chunk that can fit len_padding
                let len_padding_new_chunk = PACKET_SIZE - (PADDING_INDICATOR_SIZE + AEAD_TAG_SIZE);
                padding_len = len_padding_on_first_chunk + len_padding_new_chunk;
                // Extend by another packet size. End of packet will be modified later
                msg.extend([0 as u8; PACKET_SIZE -  AEAD_TAG_SIZE]);

            } else {
                // This case, there is room for 2 + 16 bytes at end. No need
                // to do anything else here
                padding_len = (PACKET_SIZE - AEAD_TAG_SIZE) - last_chunk_size;
            }

            let msg_len = msg.len();

            // Remove AEAD_TAG_SIZE bytes off the end (guaranteed last 18 bytes
            // are 0 at this point)
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


            // No AAD
            let msg_encrypted: Vec<u8> = sender_context.seal(
                b"", &msg).unwrap();

            println!("[Node A] Unencrypted size: {msg_len} bytes");
            println!("[Node A] Unencrypted hash: {}", calc_hash(&msg));
            println!("[Node A] Encrypted size: {} bytes", msg_encrypted.len());
            println!("[Node A] Encrypted hash: {}", calc_hash(&msg_encrypted));
            println!("[Node A] Number of padding bytes: {padding_len}");


            let num_packets = msg_encrypted.len() / PACKET_SIZE;
            //let mut send_stream = TcpStream::connect(format!("127.0.0.1:{}", server_listen_port)).unwrap();
            println!("[Node A] Sending {num_packets} packets");
            for i in 0..num_packets {
                let start_index = i * PACKET_SIZE;
                let end_index = (i + 1) * PACKET_SIZE;
                send_stream.write(&msg_encrypted[start_index..end_index]).unwrap();
            }
            //send_stream.shutdown(std::net::Shutdown::Both).unwrap();

        }

    }

}
