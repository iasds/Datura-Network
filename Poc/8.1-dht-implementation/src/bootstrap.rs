use std::sync::{Arc, Mutex};

use crate::{
    client,
    routing::{Peer, RoutingTable},
    rpc::Message,
};

// Attempt to join the DHT by introducing this node to a known bootstrap peer.
pub async fn join_network(bootstrap: String, local_peer: Peer, routing: Arc<Mutex<RoutingTable>>) {
    // Send a greeting to the bootstrap node so it can recognize this peer.
    let reply = client::rpc(&routing, &bootstrap, Message::Hello { peer: local_peer }).await;

    // If the bootstrap node accepts the connection, it returns a welcome message.
    if let Some(Message::HelloAck { peer }) = reply {
        println!("Connected to {:x?} at {}", peer.id, peer.address);

        // Remember the newly discovered peer so future lookups can use it.
        routing.lock().unwrap().add_peer(peer);
    }
}
