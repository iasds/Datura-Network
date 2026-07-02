mod identity;
mod routing;
mod records;
mod storage;
mod rpc;
mod network;
mod bootstrap;
mod client;
mod dht;
mod lookup;

use std::sync::{Arc, Mutex};
use sha2::{Sha256, Digest};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use hex::FromHex;

use identity::Identity;
use routing::RoutingTable;
use storage::Storage;
use routing::Peer;
use records::NatRecord;
use records::HSRecord;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let first_node_id = [0x13, 0x9e, 0x39, 0x40, 0xe6, 0x4b, 0x54, 0x91, 0x72, 0x20, 0x88, 0xd9, 0xa0, 0xd7, 0x41, 0x62, 0x8f, 0xc8, 0x26, 0xe0, 0x94, 0x75, 0xd3, 0x41, 0xa7, 0x80, 0xac, 0xde, 0x3c, 0x4b, 0x80, 0x70];
    let first_node_addr = "127.0.0.1:9000";

    let routing =
        Arc::new(
            Mutex::new(
                RoutingTable::new()
            )
        );

    let storage =
        Arc::new(
            Mutex::new(
                Storage::new()
            )
        );

    let args: Vec<String> =
        std::env::args().collect();

    let mode =
        args.get(1)
            .map(String::as_str)
            .unwrap_or("node");

    match mode {

        "init" => { // first node in the network, no bootstrap
            let node = Identity::init_with_value(0);
            node.print_info(); // Should print "first_node_id" exactly.
            let me = Peer {
                id: node.node_id,
                addr: first_node_addr.parse()?,
            };
            routing.lock().unwrap().set_local_peer(me);

            network::run_server(
                &first_node_addr,
                node.node_id,
                routing.clone(),
                storage.clone(),
            )
            .await?;
        }

        "node" => {

            let node = Identity::new();
            node.print_info();

            let bind =
                args
                    .get(2)
                    .cloned()
                    .unwrap_or(
                        "127.0.0.1:9001".into()
                    );
            
            let me = Peer {
                id: node.node_id,
                addr: bind.parse()?,
            };
            routing.lock().unwrap().set_local_peer(me);

            let bootstrap_id_string: String =
            args.get(3).cloned().unwrap_or(hex::encode(first_node_id));
            
            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Error parsing hex: {}. Ensure it is exactly 64 characters long.", e);
                    std::process::exit(1);
                }
            };
            
            let bootstrap_addr: Option<String> =
                args.get(4).cloned().unwrap_or(first_node_addr.to_string()).into();

            let me = Peer {
                id: node.node_id,
                addr: bind.parse()?,
            };

            if let Some(addr) = bootstrap_addr {

                println!("Joining network via {:x?} at {}", bootstrap_id, addr);

                bootstrap::join_network(
                    addr.clone(),
                    me,
                    routing.clone(),
                )
                .await;
            }

            network::run_server(
                &bind,
                node.node_id,
                routing.clone(),
                storage.clone(),
            )
            .await?;
        }

        "publish-nat" => {

            let gateway_id_string: String =
                args.get(2).cloned().unwrap_or(hex::encode(first_node_id));

            let gateway_id: [u8; 32] = match <[u8; 32]>::from_hex(gateway_id_string) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Error parsing hex: {}. Ensure it is exactly 64 characters long.", e);
                    std::process::exit(1);
                }
            };

            let gateway_addr: String =
                args.get(3).cloned().unwrap_or(first_node_addr.to_string());

            let bootstrap_id_string: String =
                args.get(4).cloned().unwrap_or(hex::encode(first_node_id));

            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Error parsing hex: {}. Ensure it is exactly 64 characters long.", e);
                    std::process::exit(1);
                }
            };

            let bootstrap_addr: String =
                args.get(5).cloned().unwrap_or(first_node_addr.to_string());

            let identity_behind_nat: Identity = Identity::new();

            let tomorrow: SystemTime = SystemTime::now() + Duration::from_secs(24 * 60 * 60);
            let tomorrow_timestamp: u64 = tomorrow
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();

            let record = NatRecord {
                owner: identity_behind_nat.node_id,
                gateway: gateway_id,
                external_address: gateway_addr.parse()?,
                timestamp: tomorrow_timestamp,
            };

            let bootstrap_peer = Peer {
                id: bootstrap_id,
                addr: bootstrap_addr.parse()?,
            };

            println!("Publishing NAT record for {:x?}", identity_behind_nat.node_id);

            dht::publish_nat_record(
                bootstrap_peer,
                record,
                routing.clone()
            )
            .await;
        }

        "resolve-nat" => {

            let node_behind_nat_id_string: String =
                args.get(2).cloned().unwrap_or(hex::encode(first_node_id));

            let node_behind_nat_id: [u8; 32] = match <[u8; 32]>::from_hex(node_behind_nat_id_string) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Error parsing hex: {}. Ensure it is exactly 64 characters long.", e);
                    std::process::exit(1);
                }
            };

            let bootstrap_id_string: String =
                args.get(3).cloned().unwrap_or(hex::encode(first_node_id));

            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Error parsing hex: {}. Ensure it is exactly 64 characters long.", e);
                    std::process::exit(1);
                }
            };

            let bootstrap_addr: String =
                args.get(4).cloned().unwrap_or(first_node_addr.to_string());

            let bootstrap_peer = Peer {
                id: bootstrap_id,
                addr: bootstrap_addr.parse()?,
            };

            let result =
                dht::resolve_nat_record(
                    bootstrap_peer,
                    node_behind_nat_id,
                    routing.clone()
                )
                .await;

            println!("{:#?}", result);
        }

        "publish-hs" => {

            let bootstrap_id_string: String =
                args.get(2).cloned().unwrap_or(hex::encode(first_node_id));

            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Error parsing hex: {}. Ensure it is exactly 64 characters long.", e);
                    std::process::exit(1);
                }
            };

            let bootstrap_addr: String =
                args.get(3).cloned().unwrap_or(first_node_addr.to_string());    

            let hs: Identity = Identity::new();
            let hs_hash: [u8; 32] = Sha256::digest(hs.get_address().as_bytes()).into();
                
            let rendezvous: [u8; 32] = Identity::new().node_id;

            let tomorrow: SystemTime = SystemTime::now() + Duration::from_secs(24 * 60 * 60);
            let tomorrow_timestamp: u64 = tomorrow
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();

            let descriptor: HSRecord = HSRecord {
                hs_hash,
                rendezvous,
                expires: tomorrow_timestamp,
            };

            let bootstrap_peer: Peer = Peer {
                id: bootstrap_id,
                addr: bootstrap_addr.parse()?,
            };

            println!("Publishing HS descriptor with hash: {}", hex::encode(hs_hash));
            println!("Node ID: {:x?}", hs.node_id);
            println!("Address: {}.dn", hs.get_address());

            dht::publish_hs_descriptor(
                bootstrap_peer,
                descriptor,
                routing.clone()
            )
            .await;

        }

        "resolve-hs" => {

            let hs_id_string: String =
                args.get(2).cloned().unwrap_or(hex::encode(first_node_id));

            let hs_id: [u8; 32] = match <[u8; 32]>::from_hex(hs_id_string) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Error parsing hex: {}. Ensure it is exactly 64 characters long.", e);
                    std::process::exit(1);
                }
            };

            let bootstrap_id_string: String =
                args.get(3).cloned().unwrap_or(hex::encode(first_node_id));

            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Error parsing hex: {}. Ensure it is exactly 64 characters long.", e);
                    std::process::exit(1);
                }
            };

            let bootstrap_addr: String =
                args.get(4).cloned().unwrap_or(first_node_addr.to_string());

            let bootstrap_peer = Peer {
                id: bootstrap_id,
                addr: bootstrap_addr.parse()?,
            };

            let result: Option<HSRecord> =
                dht::resolve_hs_descriptor(
                    bootstrap_peer,
                    hs_id,
                    routing.clone()
                )
                .await;

            println!("{:#?}", result);
        }

        _ => {}
    }

    Ok(())
}