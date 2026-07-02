use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use crate::{
    client, identity::NodeId, routing::{self, Peer, RoutingTable}, rpc::Message,
};

/// Kademlia lookup.
///
/// Starts from one bootstrap node and walks
/// the network until no closer peers can be
/// discovered.
///
/// Returns the K closest peers found.
pub async fn find_node(
    routing: Arc<Mutex<RoutingTable>>,
    bootstrap: Peer,
    target: NodeId,
    k: usize,
) -> Vec<Peer> {

    //-------------------------------------------------
    // known peers
    //-------------------------------------------------

    let mut known: HashMap<NodeId, Peer> =
        HashMap::new();

    known.insert(
        bootstrap.id,
        bootstrap.clone(),
    );

    //-------------------------------------------------
    // queried peers
    //-------------------------------------------------

    let mut queried =
        HashSet::<NodeId>::new();

    //-------------------------------------------------
    // previous best distance
    //-------------------------------------------------

    let mut previous_best: Option<[u8;32]> = None;

    //-------------------------------------------------
    // iterate
    //-------------------------------------------------

    loop {

        //-------------------------------------------------
        // choose nearest unqueried peer
        //-------------------------------------------------

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

        //-------------------------------------------------
        // ask peer
        //-------------------------------------------------

        let reply =
            client::rpc(
                &mut routing.lock().unwrap(),
                &peer.addr.to_string(),
                Message::FindNode {
                    target,
                },
            )
            .await;

        //-------------------------------------------------
        // ignore failures
        //-------------------------------------------------

        let Some(
            Message::Nodes { peers }
        ) = reply else {
            continue;
        };

        //-------------------------------------------------
        // merge
        //-------------------------------------------------

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

        //-------------------------------------------------
        // convergence
        //-------------------------------------------------

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

    //-------------------------------------------------
    // return K closest
    //-------------------------------------------------

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

///
/// Find nearest peer not already queried.
///
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
