use std::sync::{Arc, Mutex};

use tokio::net::UdpSocket;

use crate::{
    routing::{Peer, RoutingTable},
    rpc::Message,
};

/// Opens a temporary local socket, sends the request, reads the response over UDP
/// and updates the local routing table with anything useful that came back in the reply.
///
/// The routing table lock is only ever taken for the brief, synchronous update at the end,
/// never across the network `.await` points, so a slow or unresponsive peer cannot block
/// other tasks that need the routing table.
pub async fn rpc(
    routing: &Arc<Mutex<RoutingTable>>,
    destination: &str,
    request_message: Message,
) -> Option<Message> {
    // Bind to an ephemeral local port.
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;

    // Turn the message into bytes.
    let bytes = serde_json::to_vec(&request_message).ok()?;

    // Send the request off to the target address.
    socket.send_to(&bytes, destination).await.ok()?;

    // Give the response a little room to land.
    let mut response_buffer = [0u8; 4096];

    // Wait for the remote peer to answer.
    let (response_length, _) = socket.recv_from(&mut response_buffer).await.ok()?;

    // Deserialize the reply back into a typed message.
    let reply: Message = serde_json::from_slice(&response_buffer[..response_length]).ok()?;

    // If the reply contains peer information, remember it for later lookups.
    match &reply {
        Message::Pong {
            id: responder_node_id,
            peer,
        } => {
            // We learned about the peer that answered and the node behind the ID.
            let mut routing_table = routing.lock().unwrap();
            routing_table.add_peer(peer.clone());

            routing_table.add_peer(Peer {
                id: *responder_node_id,
                address: destination.parse().ok()?,
            });
        }

        Message::HelloAck { peer } => {
            // A simple acknowledgement tells us the peer exists.
            routing.lock().unwrap().add_peer(peer.clone());
        }

        Message::Nodes { peers } => {
            // The remote peer shares a list of known nodes, so we can grow the routing table.
            let mut routing_table = routing.lock().unwrap();
            for discovered_peer in peers {
                routing_table.add_peer(discovered_peer.clone());
            }
        }

        _ => {}
    }

    Some(reply)
}
