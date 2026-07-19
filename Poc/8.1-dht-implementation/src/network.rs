use std::{
    sync::{Arc, Mutex},
};

use tokio::net::UdpSocket;

use crate::{
    identity::NodeId, routing::{Peer, RoutingTable}, rpc::Message, storage::Storage,
};

/// This is the node's network loop: it listens for incoming messages and updates local state.
pub async fn run_server(
    bind_addr: &str,
    my_id: NodeId,
    routing: Arc<Mutex<RoutingTable>>,
    storage: Arc<Mutex<Storage>>,
) -> Result<(), Box<dyn std::error::Error>> {

    println!("Starting server with ID {:x?}", my_id);

    let socket = UdpSocket::bind(bind_addr).await?;
    let my_peer = Peer {
        id: my_id,
        addr: socket.local_addr()?,
    };

    println!("Listening on {}", bind_addr);

    let mut buffer = [0u8; 4096];

    // Simple server for PoC: each packet is handled as an isolated request,
    // and the response is built from whatever local state the node already has.
    loop {

        let (size, sender) =
            socket.recv_from(&mut buffer).await?;

        let msg: Message =
            serde_json::from_slice(&buffer[..size])?;


        println!("Received {:x?} from {}", msg, sender);


        let reply = match msg {

            Message::Ping => {
                Some(Message::Pong {
                    id: my_id,
                    peer: my_peer.clone(),
                })
            }

            Message::Hello { peer } => {
                // Learning about a new peer is the basic way the network grows.
                routing.lock().unwrap().add_peer(peer.clone());

                Some(Message::HelloAck {
                    peer: my_peer.clone(),
                })
            }

            Message::FindNode { target } => {
                // Local routing table lookup.
                let peers =
                    routing
                        .lock()
                        .unwrap()
                        .closest(target, 16);

                Some(Message::Nodes { peers })
            }

            Message::Store { key, record } => {
                // Storing data.
                storage.lock().unwrap().put(key, record);

                None
            }

            Message::FindValue { key } => {
                // Returns a stored record.
                let value =
                    storage.lock().unwrap().get(&key).cloned();

                Some(Message::Value { record: value })
            }

            _ => None,
        };


        if let Some(reply) = reply {

            let bytes =
                serde_json::to_vec(&reply)?;

            socket
                .send_to(
                    &bytes,
                    sender,
                )
                .await?;
        }
    }
}