use std::sync::{Arc, Mutex};

use tokio::net::UdpSocket;

use crate::{
    identity::NodeId,
    routing::{Peer, RoutingTable},
    rpc::Message,
    storage::Storage,
};

/// This is the node's network loop: it listens for incoming messages and updates local state.
pub async fn run_server(
    bind_address: &str,
    local_node_id: NodeId,
    routing: Arc<Mutex<RoutingTable>>,
    storage: Arc<Mutex<Storage>>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting server with ID {:x?}", local_node_id);

    let socket = UdpSocket::bind(bind_address).await?;
    let local_peer = Peer {
        id: local_node_id,
        address: socket.local_addr()?,
    };

    println!("Listening on {}", bind_address);

    let mut receive_buffer = [0u8; 4096];

    // Simple server for PoC: each packet is handled as an isolated request,
    // and the response is built from whatever local state the node already has.
    loop {
        let (received_size, sender_address) = socket.recv_from(&mut receive_buffer).await?;

        let request_message: Message = serde_json::from_slice(&receive_buffer[..received_size])?;

        println!("Received {:x?} from {}", request_message, sender_address);

        let reply = match request_message {
            Message::Ping => Some(Message::Pong {
                id: local_node_id,
                peer: local_peer.clone(),
            }),

            Message::Hello { peer } => {
                // Learning about a new peer is the basic way the network grows.
                routing.lock().unwrap().add_peer(peer.clone());

                Some(Message::HelloAck {
                    peer: local_peer.clone(),
                })
            }

            Message::FindNode { target } => {
                // Local routing table lookup.
                let peers = routing.lock().unwrap().closest(target, 16);

                Some(Message::Nodes { peers })
            }

            Message::Store { key, record } => {
                // Storing data.
                storage.lock().unwrap().put(key, record);

                None
            }

            Message::FindValue { key } => {
                // Returns a stored record.
                let value = storage.lock().unwrap().get(&key).cloned();

                Some(Message::Value { record: value })
            }

            _ => None,
        };

        if let Some(reply) = reply {
            let reply_bytes = serde_json::to_vec(&reply)?;

            socket.send_to(&reply_bytes, sender_address).await?;
        }
    }
}
