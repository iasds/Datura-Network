use std::sync::{Arc, Mutex};

use crate::{
    client,
    identity::NodeId,
    records::HSRecord,
    records::NatRecord,
    records::Record,
    rpc::Message,
    routing::Peer,
    routing::RoutingTable,
};

pub async fn find_closest(
    bootstrap: Peer,
    target: NodeId,
    routing: Arc<Mutex<RoutingTable>>,
) -> Vec<Peer>
{
    match client::rpc(
        &mut routing.lock().unwrap(),
        &bootstrap.addr.to_string(),
        Message::FindNode {
            target,
        },
    )
    .await
    {
        Some(
            Message::Nodes {
                peers,
            },
        ) => peers,

        _ => vec![],
    }
}

pub async fn publish_nat_record(
    bootstrap: Peer,
    record: NatRecord,
    routing: Arc<Mutex<RoutingTable>>,
)
{
    let routing_for_lookup: Arc<Mutex<RoutingTable>> = Arc::clone(&routing);
    let peers: Vec<Peer> =
        find_closest(
            bootstrap,
            record.owner,
            routing_for_lookup,
        )
        .await;

    for peer in peers {

        let _ =
            client::rpc(
                &mut routing.lock().unwrap(),
                &peer.addr.to_string(),
                Message::Store {
                    key: record.owner,
                    record: Record::Nat(
                        record.clone(),
                    ),
                },
            )
            .await;
    }
}

pub async fn resolve_nat_record(
    bootstrap: Peer,
    node_id: NodeId,
    routing: Arc<Mutex<RoutingTable>>,
) -> Option<NatRecord>
{
    let routing_for_lookup: Arc<Mutex<RoutingTable>> = Arc::clone(&routing);
    let peers =
        find_closest(
            bootstrap,
            node_id,
            routing_for_lookup,
        )
        .await;

    for peer in peers {

        if let Some(
            Message::Value {
                record:
                    Some(
                        Record::Nat(r),
                    ),
            },
        ) =
            client::rpc(
                &mut routing.lock().unwrap(),
                &peer.addr.to_string(),
                Message::FindValue {
                    key: node_id,
                },
            )
            .await
        {
            return Some(r);
        }
    }

    None
}

pub async fn publish_hs_descriptor(
    bootstrap: Peer,
    record: HSRecord,
    routing: Arc<Mutex<RoutingTable>>,
)
{
    let routing_for_lookup: Arc<Mutex<RoutingTable>> = Arc::clone(&routing);
    let peers =
        find_closest(
            bootstrap,
            record.hs_hash,
            routing_for_lookup,
        )
        .await;

    for peer in peers {

        let _ =
            client::rpc(
                &mut routing.lock().unwrap(),
                &peer.addr.to_string(),
                Message::Store {
                    key: record.hs_hash,
                    record:
                        Record::HiddenService(
                            record.clone(),
                        ),
                },
            )
            .await;
    }
}

pub async fn resolve_hs_descriptor(
    bootstrap: Peer,
    hs_hash: NodeId,
    routing: Arc<Mutex<RoutingTable>>,
) -> Option<HSRecord>
{
    let routing_for_lookup: Arc<Mutex<RoutingTable>> = Arc::clone(&routing);
    let peers: Vec<Peer> =
        find_closest(
            bootstrap,
            hs_hash,
            routing_for_lookup,
        )
        .await;

    for peer in peers {

        if let Some(
            Message::Value {
                record:
                    Some(
                        Record::HiddenService(
                            desc,
                        ),
                    ),
            },
        ) =
            client::rpc(
                &mut routing.lock().unwrap(),
                &peer.addr.to_string(),
                Message::FindValue {
                    key: hs_hash,
                },
            )
            .await
        {
            return Some(desc);
        }
    }

    None
}

