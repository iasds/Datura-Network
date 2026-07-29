mod bootstrap;
mod client;
mod dht;
mod identity;
mod lookup;
mod network;
mod records;
mod routing;
mod rpc;
mod storage;

use hex::FromHex;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use identity::Identity;
use records::HSRecord;
use records::NatRecord;
use routing::Peer;
use routing::RoutingTable;
use storage::Storage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The entrypoint acts as a thin CLI wrapper around the node, bootstrap, and DHT operations.

    let first_node_id = [
        0x13, 0x9e, 0x39, 0x40, 0xe6, 0x4b, 0x54, 0x91, 0x72, 0x20, 0x88, 0xd9, 0xa0, 0xd7, 0x41,
        0x62, 0x8f, 0xc8, 0x26, 0xe0, 0x94, 0x75, 0xd3, 0x41, 0xa7, 0x80, 0xac, 0xde, 0x3c, 0x4b,
        0x80, 0x70,
    ];
    let first_node_address = "127.0.0.1:9000";

    let routing = Arc::new(Mutex::new(RoutingTable::new()));

    let storage = Arc::new(Mutex::new(Storage::new()));

    let args: Vec<String> = std::env::args().collect();

    let mode = args.get(1).map(String::as_str).unwrap_or("node");

    match mode {
        "init" => {
            // Start the first node in the network without needing any bootstrap peer.
            let node = Identity::init_with_value(0);
            node.print_info(); // Should print "first_node_id" exactly.
            let local_peer = Peer {
                id: node.node_id,
                address: first_node_address.parse()?,
            };
            routing.lock().unwrap().set_local_peer(local_peer);

            network::run_server(
                first_node_address,
                node.node_id,
                routing.clone(),
                storage.clone(),
            )
            .await?;
        }

        "node" => {
            // Regular nodes join an existing network and then start serving requests.

            let node = Identity::new();
            node.print_info();

            let bind = args.get(2).cloned().unwrap_or("127.0.0.1:9001".into());

            let local_peer = Peer {
                id: node.node_id,
                address: bind.parse()?,
            };
            routing.lock().unwrap().set_local_peer(local_peer.clone());

            let bootstrap_id_string: String =
                args.get(3).cloned().unwrap_or(hex::encode(first_node_id));

            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(decoded_bytes) => decoded_bytes,
                Err(parse_error) => {
                    eprintln!(
                        "Error parsing hex: {}. Ensure it is exactly 64 characters long.",
                        parse_error
                    );
                    std::process::exit(1);
                }
            };

            let bootstrap_address: Option<String> = args
                .get(4)
                .cloned()
                .unwrap_or(first_node_address.to_string())
                .into();

            if let Some(resolved_bootstrap_address) = bootstrap_address {
                println!(
                    "Joining network via {:x?} at {}",
                    bootstrap_id, resolved_bootstrap_address
                );

                bootstrap::join_network(
                    resolved_bootstrap_address.clone(),
                    local_peer,
                    routing.clone(),
                )
                .await;
            }

            network::run_server(&bind, node.node_id, routing.clone(), storage.clone()).await?;
        }

        "publish-nat" => {
            // Publish a NAT record so other nodes can find the gateway for a peer behind NAT.

            let gateway_id_string: String =
                args.get(2).cloned().unwrap_or(hex::encode(first_node_id));

            let gateway_id: [u8; 32] = match <[u8; 32]>::from_hex(gateway_id_string) {
                Ok(decoded_bytes) => decoded_bytes,
                Err(parse_error) => {
                    eprintln!(
                        "Error parsing hex: {}. Ensure it is exactly 64 characters long.",
                        parse_error
                    );
                    std::process::exit(1);
                }
            };

            let gateway_address: String = args
                .get(3)
                .cloned()
                .unwrap_or(first_node_address.to_string());

            let bootstrap_id_string: String =
                args.get(4).cloned().unwrap_or(hex::encode(first_node_id));

            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(decoded_bytes) => decoded_bytes,
                Err(parse_error) => {
                    eprintln!(
                        "Error parsing hex: {}. Ensure it is exactly 64 characters long.",
                        parse_error
                    );
                    std::process::exit(1);
                }
            };

            let bootstrap_address: String = args
                .get(5)
                .cloned()
                .unwrap_or(first_node_address.to_string());

            let identity_behind_nat: Identity = Identity::new();

            let tomorrow: SystemTime = SystemTime::now() + Duration::from_secs(24 * 60 * 60);
            let tomorrow_timestamp: u64 = tomorrow
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();

            let record = NatRecord {
                owner: identity_behind_nat.node_id,
                gateway: gateway_id,
                external_address: gateway_address.parse()?,
                timestamp: tomorrow_timestamp,
            };

            let bootstrap_peer = Peer {
                id: bootstrap_id,
                address: bootstrap_address.parse()?,
            };

            println!(
                "Publishing NAT record for {:x?}",
                identity_behind_nat.node_id
            );

            dht::publish_nat_record(bootstrap_peer, record, routing.clone()).await;
        }

        "resolve-nat" => {
            // Look up a previously published NAT record using the DHT.

            let node_behind_nat_id_string: String =
                args.get(2).cloned().unwrap_or(hex::encode(first_node_id));

            let node_behind_nat_id: [u8; 32] = match <[u8; 32]>::from_hex(node_behind_nat_id_string)
            {
                Ok(decoded_bytes) => decoded_bytes,
                Err(parse_error) => {
                    eprintln!(
                        "Error parsing hex: {}. Ensure it is exactly 64 characters long.",
                        parse_error
                    );
                    std::process::exit(1);
                }
            };

            let bootstrap_id_string: String =
                args.get(3).cloned().unwrap_or(hex::encode(first_node_id));

            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(decoded_bytes) => decoded_bytes,
                Err(parse_error) => {
                    eprintln!(
                        "Error parsing hex: {}. Ensure it is exactly 64 characters long.",
                        parse_error
                    );
                    std::process::exit(1);
                }
            };

            let bootstrap_address: String = args
                .get(4)
                .cloned()
                .unwrap_or(first_node_address.to_string());

            let bootstrap_peer = Peer {
                id: bootstrap_id,
                address: bootstrap_address.parse()?,
            };

            let result =
                dht::resolve_nat_record(bootstrap_peer, node_behind_nat_id, routing.clone()).await;

            println!("{:#?}", result);
        }

        "publish-hs" => {
            // Publish a hidden service descriptor so it can be discovered later by hash.

            let bootstrap_id_string: String =
                args.get(2).cloned().unwrap_or(hex::encode(first_node_id));

            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(decoded_bytes) => decoded_bytes,
                Err(parse_error) => {
                    eprintln!(
                        "Error parsing hex: {}. Ensure it is exactly 64 characters long.",
                        parse_error
                    );
                    std::process::exit(1);
                }
            };

            let bootstrap_address: String = args
                .get(3)
                .cloned()
                .unwrap_or(first_node_address.to_string());

            let hidden_service_identity: Identity = Identity::new();
            let hidden_service_hash: [u8; 32] =
                Sha256::digest(hidden_service_identity.get_address().as_bytes()).into();

            let rendezvous: [u8; 32] = Identity::new().node_id;

            let tomorrow: SystemTime = SystemTime::now() + Duration::from_secs(24 * 60 * 60);
            let tomorrow_timestamp: u64 = tomorrow
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();

            let descriptor: HSRecord = HSRecord {
                hidden_service_hash,
                rendezvous,
                expires: tomorrow_timestamp,
            };

            let bootstrap_peer: Peer = Peer {
                id: bootstrap_id,
                address: bootstrap_address.parse()?,
            };

            println!(
                "Publishing HS descriptor with hash: {}",
                hex::encode(hidden_service_hash)
            );
            println!("Node ID: {:x?}", hidden_service_identity.node_id);
            println!("Address: {}.dn", hidden_service_identity.get_address());

            dht::publish_hs_descriptor(bootstrap_peer, descriptor, routing.clone()).await;
        }

        "resolve-hs" => {
            // Resolve a hidden service descriptor from the DHT.

            let hidden_service_id_string: String =
                args.get(2).cloned().unwrap_or(hex::encode(first_node_id));

            let hidden_service_id: [u8; 32] = match <[u8; 32]>::from_hex(hidden_service_id_string) {
                Ok(decoded_bytes) => decoded_bytes,
                Err(parse_error) => {
                    eprintln!(
                        "Error parsing hex: {}. Ensure it is exactly 64 characters long.",
                        parse_error
                    );
                    std::process::exit(1);
                }
            };

            let bootstrap_id_string: String =
                args.get(3).cloned().unwrap_or(hex::encode(first_node_id));

            let bootstrap_id: [u8; 32] = match <[u8; 32]>::from_hex(bootstrap_id_string) {
                Ok(decoded_bytes) => decoded_bytes,
                Err(parse_error) => {
                    eprintln!(
                        "Error parsing hex: {}. Ensure it is exactly 64 characters long.",
                        parse_error
                    );
                    std::process::exit(1);
                }
            };

            let bootstrap_address: String = args
                .get(4)
                .cloned()
                .unwrap_or(first_node_address.to_string());

            let bootstrap_peer = Peer {
                id: bootstrap_id,
                address: bootstrap_address.parse()?,
            };

            let result: Option<HSRecord> =
                dht::resolve_hs_descriptor(bootstrap_peer, hidden_service_id, routing.clone())
                    .await;

            println!("{:#?}", result);
        }

        _ => {}
    }

    Ok(())
}
