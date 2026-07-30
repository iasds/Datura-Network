use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use crate::{
    client,
    identity::NodeId,
    routing::{self, Peer, RoutingTable},
    rpc::Message,
};

/// A simple Kademlia implementation, good enough for a PoC, but not production.
/// Walks the network outward from a bootstrap peer until we have a short list of nodes
/// that are closest to the requested target.
pub async fn find_node(
    routing: Arc<Mutex<RoutingTable>>,
    bootstrap: Peer,
    target: NodeId,
    result_count: usize,
) -> Vec<Peer> {
    // Keep track of every peer we have seen so far, using the node ID as the lookup key.
    // This lets the search avoid repeatedly asking the same node and makes the nearest-peer
    // selection deterministic enough to be useful.
    let mut known_peers: HashMap<NodeId, Peer> = HashMap::new();

    known_peers.insert(bootstrap.id, bootstrap.clone());

    // Once a peer has been queried, we do not want to keep hammering it for the same lookup.
    let mut queried_peer_ids = HashSet::<NodeId>::new();

    let mut previous_best_distance: Option<[u8; 32]> = None;

    loop {
        // Pick the closest peer we have not asked yet. That keeps the search focused on the
        // part of the network that matters instead of wandering blindly.
        let next_peer = nearest_unqueried(&known_peers, &queried_peer_ids, &target);

        let Some(peer) = next_peer else {
            break;
        };

        queried_peer_ids.insert(peer.id);

        // The routing table lock is intentionally not held across this `.await`: `client::rpc`
        // takes the shared handle and only locks briefly, internally, once the reply arrives.
        let reply = client::rpc(
            &routing,
            &peer.address.to_string(),
            Message::FindNode { target },
        )
        .await;

        let Some(Message::Nodes {
            peers: discovered_peers,
        }) = reply
        else {
            continue;
        };

        let mut found_new_peer = false;

        {
            let mut routing_table = routing.lock().unwrap();

            for discovered_peer in discovered_peers {
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    known_peers.entry(discovered_peer.id)
                {
                    routing_table.add_peer(discovered_peer.clone());
                    entry.insert(discovered_peer);
                    found_new_peer = true;
                }
            }
        }

        // If the latest round brought in fresh peers, we should keep going; otherwise the
        // search has likely reached the edge of what this neighborhood can tell us.
        let closest_known_peer = known_peers
            .values()
            .min_by(|first, second| {
                routing::xor_distance(&first.id, &target)
                    .cmp(&routing::xor_distance(&second.id, &target))
            })
            .unwrap();

        let closest_known_distance = routing::xor_distance(&closest_known_peer.id, &target);

        if let Some(previous_distance) = previous_best_distance
            && closest_known_distance == previous_distance
            && !found_new_peer
        {
            break;
        }

        previous_best_distance = Some(closest_known_distance);
    }

    // By the time the loop finishes, we have a broad view of the nearby region and can return
    // the best K candidates.
    let mut peers = known_peers.into_values().collect::<Vec<_>>();

    peers.sort_by(|first, second| {
        routing::xor_distance(&first.id, &target).cmp(&routing::xor_distance(&second.id, &target))
    });

    peers.truncate(result_count);

    peers
}

/// Chooses the unqueried peer that is currently closest to the target.
/// This is the small piece of logic used in `find_node` function
/// that keeps the lookup converging toward the right part of the network.
fn nearest_unqueried(
    known_peers: &HashMap<NodeId, Peer>,
    queried_peer_ids: &HashSet<NodeId>,
    target: &NodeId,
) -> Option<Peer> {
    known_peers
        .values()
        .filter(|peer| !queried_peer_ids.contains(&peer.id))
        .min_by(|first, second| {
            routing::xor_distance(&first.id, target).cmp(&routing::xor_distance(&second.id, target))
        })
        .cloned()
}
