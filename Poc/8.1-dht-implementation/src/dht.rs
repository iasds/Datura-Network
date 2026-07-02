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
    lookup,
};

pub async fn publish_nat_record(
    bootstrap: Peer,
    record: NatRecord,
    routing: Arc<Mutex<RoutingTable>>,
)
{
    let close_peers = lookup::find_node(
        routing.clone(),
        bootstrap.clone(),
        record.owner,
        16,
    ).await;

    for peer in close_peers {

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

    let close_peers = lookup::find_node(
        routing.clone(),
        bootstrap.clone(),
        node_id,
        16,
    ).await;

    for peer in close_peers {

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
    let close_peers = lookup::find_node(
        routing.clone(),
        bootstrap.clone(),
        record.hs_hash,
        16,
    ).await;

    for peer in close_peers {

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
    let close_peers = lookup::find_node(
        routing.clone(),
        bootstrap.clone(),
        hs_hash,
        16,
    ).await;

    for peer in close_peers {

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

