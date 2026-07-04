use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use crate::{
    client, identity::NodeId, routing::{self, Peer, RoutingTable}, rpc::Message,
};

/// A simple Kademlia implementation, good enough for a PoC, but not production.
/// Walks the network outward from a bootstrap peer until we have a short list of nodes
/// that are closest to the requested target.
pub async fn find_node(
    routing: Arc<Mutex<RoutingTable>>,
    bootstrap: Peer,
    target: NodeId,
    k: usize,
) -> Vec<Peer> {

    // Keep track of every peer we have seen so far, using the node ID as the lookup key.
    // This lets the search avoid repeatedly asking the same node and makes the nearest-peer
    // selection deterministic enough to be useful.
    let mut known: HashMap<NodeId, Peer> =
        HashMap::new();

    known.insert(
        bootstrap.id,
        bootstrap.clone(),
    );

    // Once a peer has been queried, we do not want to keep hammering it for the same lookup.
    let mut queried =
        HashSet::<NodeId>::new();

    let mut previous_best: Option<[u8;32]> = None;

    loop {
        // Pick the closest peer we have not asked yet. That keeps the search focused on the
        // part of the network that matters instead of wandering blindly.

        let next =
            nearest_unqueried(
                &known,
                &queried,
                &target,
            );

        let Some(peer) = next else {
            break;
        };

        queried.insert(peer.id);

        let reply =
            client::rpc(
                &mut routing.lock().unwrap(),
                &peer.addr.to_string(),
                Message::FindNode {
                    target,
                },
            )
            .await;

        let Some(
            Message::Nodes { peers }
        ) = reply else {
            continue;
        };

        let mut discovered = false;

        {
            let mut rt =
                routing.lock().unwrap();

            for p in peers {

                if !known.contains_key(&p.id) {

                    rt.add_peer(p.clone());

                    known.insert(
                        p.id,
                        p,
                    );

                    discovered = true;
                }
            }
        }

        // If the latest round brought in fresh peers, we should keep going; otherwise the
        // search has likely reached the edge of what this neighborhood can tell us.
        let best =
            known
                .values()
                .min_by(|a,b|{

                    routing::xor_distance(
                        &a.id,
                        &target,
                    )
                    .cmp(
                        &routing::xor_distance(
                            &b.id,
                            &target,
                        )
                    )

                })
                .unwrap();

        let best_distance =
            routing::xor_distance(
                &best.id,
                &target,
            );

        if let Some(previous)
            = previous_best
        {

            if best_distance == previous
                && !discovered
            {
                break;
            }
        }

        previous_best =
            Some(best_distance);
    }

    // By the time the loop finishes, we have a broad view of the nearby region and can return
    // the best K candidates.
    let mut peers =
        known
        .into_values()
        .collect::<Vec<_>>();

    peers.sort_by(|a,b|{

        routing::xor_distance(
            &a.id,
            &target,
        )
        .cmp(
            &routing::xor_distance(
                &b.id,
                &target,
            )
        )

    });

    peers.truncate(k);

    peers
}

/// Chooses the unqueried peer that is currently closest to the target.
/// This is the small piece of logic used in `find_node` function 
/// that keeps the lookup converging toward the right part of the network.
fn nearest_unqueried(

    known: &HashMap<NodeId,Peer>,

    queried: &HashSet<NodeId>,

    target: &NodeId,

) -> Option<Peer> {

    known
        .values()
        .filter(|p|{

            !queried.contains(
                &p.id
            )

        })
        .min_by(|a,b|{

            routing::xor_distance(
                &a.id,
                target,
            )
            .cmp(
                &routing::xor_distance(
                    &b.id,
                    target,
                )
            )

        })
        .cloned()
}
