use tokio::net::UdpSocket;

use crate::{
    rpc::Message,
    routing::{Peer, RoutingTable},
};

/// Opens a temporary local socket, sends the request, reads the response over UDP
/// and updates the local routing table with anything useful that came back in the reply.
pub async fn rpc(
    routing: &mut RoutingTable,
    destination: &str,
    msg: Message,
) -> Option<Message> {
    // Bind to an ephemeral local port.
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;

    // Turn the message into bytes.
    let bytes = serde_json::to_vec(&msg).ok()?;

    // Send the request off to the target address.
    socket.send_to(&bytes, destination).await.ok()?;

    // Give the response a little room to land.
    let mut buf = [0u8; 4096];

    // Wait for the remote peer to answer.
    let (len, _) = socket.recv_from(&mut buf).await.ok()?;

    // Deserialize the reply back into a typed message.
    let reply: Message = serde_json::from_slice(&buf[..len]).ok()?;

    // If the reply contains peer information, remember it for later lookups.
    match &reply {
        Message::Pong { id, peer } => {
            // We learned about the peer that answered and the node behind the ID.
            routing.add_peer(peer.clone());

            routing.add_peer(Peer {
                id: *id,
                addr: destination.parse().ok()?,
            });
        }

        Message::HelloAck { peer } => {
            // A simple acknowledgement tells us the peer exists.
            routing.add_peer(peer.clone());
        }

        Message::Nodes { peers } => {
            // The remote peer shares a list of known nodes, so we can grow the routing table.
            for p in peers {
                routing.add_peer(p.clone());
            }
        }

        _ => {}
    }

    Some(reply)
}