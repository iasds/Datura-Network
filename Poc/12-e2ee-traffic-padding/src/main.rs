use std::{
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream}
};

use hpke_rs::{*, hpke_types::*};
use hpke_rs_libcrux::HpkeLibcrux;


// Can be changed
const MAX_PACKET_SIZE: usize = 1024; // bytes

fn usage_and_die() {
    eprintln!("Usage:");
    eprintln!();
    eprintln!("./target/debug/e2ee-traffic-padding server [server-listen-port]");
    eprintln!("./target/debug/e2ee-traffic-padding client [client-listen-port] [server-listen-port]");
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
            let mut read: Vec<u8> = vec![];

            // send pk to client
            stream.write(&pk.as_slice()).unwrap();
            println!("[Node B] Sent encapsulation key to Node A");
            // Receive cihpertext; X-Wing ciphertext len is 1120 bytes
            let mut c: Vec<u8> = vec![0; 1120];
            stream.read_exact(&mut c).unwrap();
            println!("[Node B] Received ciphertext from Node A");

            let receiver_context = hpke.setup_receiver(
                &c, &sk, b"", None, None, None);


            let now = std::time::Instant::now();
            loop {
                let mut buf: [u8; MAX_PACKET_SIZE] = [0; MAX_PACKET_SIZE];
                let s = stream.read(&mut buf).unwrap();
                if s < MAX_PACKET_SIZE {
                    break;
                }
                // Extend after, so when 0 bytes are read, then don't add that to buffer.
                read.extend(&buf);


            }

            let len = read.len();
            let top_half_len_padding = read[len - 2] as i32;
            let bottom_half_len_padding = read[len - 1] as i32;

            let len_padding: i32 = (top_half_len_padding << 8 & 0xff00) | (bottom_half_len_padding & 0xff);

            //println!("[Node B] {:?}", read);
            println!("[Node B] Read size: {}", read.len());
            println!("[Node B] Read hash: {}", calc_hash(&read));
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

            let (sk, pk) = hpke.generate_key_pair().unwrap().into_keys();

            // Connection bewteen Node A and Node B
            let mut send_stream = TcpStream::connect(format!("127.0.0.1:{}", server_listen_port)).unwrap();

            // 1216: X-Wing ecapsulation key length
            let mut server_pk_buf = [0u8; 1216];
            send_stream.read_exact(&mut server_pk_buf).unwrap();
            println!("[Node A] Received encapsulation key from Node B");

            let server_pk = HpkePublicKey::new(server_pk_buf.to_vec());
            let (c, mut sender_context) =
                hpke.setup_sender(&server_pk, b"", None, None, None).unwrap();

            // Send ciphertext to server
            send_stream.write(&c).unwrap();
            println!("[Node A] Sent ciphertext to Node B");
            //send_stream.shutdown(std::net::Shutdown::Both).unwrap();



            let mut msg = Vec::new();

            // informs for length of padding
            let last_chunk_size;
            // read data to be sent to server node b
            loop {
                let mut buf: [u8; MAX_PACKET_SIZE] = [0; MAX_PACKET_SIZE];
                let s = recv_stream.read(&mut buf).unwrap();
                msg.extend(&buf);
                if s < MAX_PACKET_SIZE {
                    last_chunk_size = s;
                    break;
                }
            }



            // Packet format:
            // data
            // padding
            // len_padding (2 bytes)

            let padding_len;
            // Need 2 bytes for len of 0 padding.
            // If < 2 bytes left at end of last chunk, need a new chunk.
            if (MAX_PACKET_SIZE - last_chunk_size) < 2 {
                // Add padding for original chunk
                let len_padding_for_first = MAX_PACKET_SIZE - last_chunk_size;
                // Need padding for another chunk that can fit len_padding
                let len_padding_additional = MAX_PACKET_SIZE - 2;
                padding_len = len_padding_for_first + len_padding_additional;
                // Since extended Vec with initialized array, the Vec already has
                // padding for current chunk. So add a new one
                msg.extend([0 as u8; MAX_PACKET_SIZE]);
            } else {
                // This case, there is room for 2 bytes at end
                padding_len = MAX_PACKET_SIZE - last_chunk_size;
            }

            let msg_len = msg.len();

            let top_half_len_padding = (padding_len >> 8 & 0xff) as u8;
            let bottom_half_len_padding = (padding_len >> 0 & 0xff) as u8;

            msg[msg_len - 2] = top_half_len_padding;
            msg[msg_len - 1] = bottom_half_len_padding;


            // No AAD
            let msg_encrypted = sender_context.seal(
                b"", &msg).unwrap();

            println!("[Node A] Send size {msg_len}");
            println!("[Node A] Send hash {}", calc_hash(&msg));
            println!("[Node A] Number of padding bytes: {padding_len}");


            let num_packets = msg.len() / MAX_PACKET_SIZE;
            //let mut send_stream = TcpStream::connect(format!("127.0.0.1:{}", server_listen_port)).unwrap();
            println!("Sending {num_packets} packets");
            for i in 0..num_packets {
                let start_index = i * MAX_PACKET_SIZE;
                let end_index = (i + 1) * MAX_PACKET_SIZE;
                send_stream.write(&msg[start_index..end_index]).unwrap();
            }
            //send_stream.shutdown(std::net::Shutdown::Both).unwrap();

        }

    }

}
