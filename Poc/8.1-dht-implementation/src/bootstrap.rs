use std::{
    sync::{Arc, Mutex},
};

use crate::{
    client,
    rpc::Message,
    routing::{Peer, RoutingTable},
};

pub async fn join_network(
    bootstrap: String,
    me: Peer,
    routing: Arc<Mutex<RoutingTable>>,
) {

    let reply =
        client::rpc(
            &mut routing.lock().unwrap(),
            &bootstrap,
            Message::Hello {
                peer: me,
            },
        )
        .await;

    if let Some(
        Message::HelloAck { peer }
    ) = reply
    {
        println!(
            "Connected to {:x?} at {}",
            peer.id, peer.addr
        );

        routing
            .lock()
            .unwrap()
            .add_peer(peer);
    }
}
