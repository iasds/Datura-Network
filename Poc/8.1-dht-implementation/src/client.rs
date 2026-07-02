use tokio::net::UdpSocket;

use crate::{
    rpc::Message,
    routing::{Peer, RoutingTable},
};

///
/// Send RPC + optionally update routing table
///
pub async fn rpc(
    routing: &mut RoutingTable,
    destination: &str,
    msg: Message,
) -> Option<Message> {

    let socket =
        UdpSocket::bind("0.0.0.0:0")
            .await
            .ok()?;

    // println!(
    //     "Sending RPC to {}: {:?}",
    //     destination, msg
    // );

    let bytes =
        serde_json::to_vec(&msg).ok()?;

    // println!(
    //     "Serialized message ({} bytes): {:?}",
    //     bytes.len(), bytes
    // );

    socket
        .send_to(&bytes, destination)
        .await
        .ok()?;

    // println!(
    //     "Sent {} bytes to {}",
    //     bytes.len(), destination
    // );

    let mut buf = [0u8; 4096];

    let (len, _) =
        socket.recv_from(&mut buf)
            .await
            .ok()?;

    let reply: Message =
        serde_json::from_slice(&buf[..len]).ok()?;

    // println!(
    //     "Received reply from {}: {:?}",
    //     destination, reply
    // );

    //-------------------------------------------------
    // AUTO PEER LEARNING
    //-------------------------------------------------

    match &reply {

        Message::Pong { id, peer } => {

            routing.add_peer(peer.clone());

            routing.add_peer(Peer {
                id: *id,
                addr: destination.parse().ok()?,
            });
        }

        Message::HelloAck { peer } => {
            routing.add_peer(peer.clone());
        }

        Message::Nodes { peers } => {
            for p in peers {
                routing.add_peer(p.clone());
            }
        }

        _ => {}
    }

    Some(reply)
}