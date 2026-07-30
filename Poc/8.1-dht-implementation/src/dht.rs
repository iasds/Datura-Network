use std::sync::{Arc, Mutex};

use crate::{
    client, identity::NodeId, lookup, records::HSRecord, records::NatRecord, records::Record,
    routing::Peer, routing::RoutingTable, rpc::Message,
};

/// Publishes a NAT record by sending it to XOR closest peers to the owner ID.
/// This is a best-effort store. We ignore any individual failures in this PoC.
pub async fn publish_nat_record(
    bootstrap: Peer,
    record: NatRecord,
    routing: Arc<Mutex<RoutingTable>>,
) {
    let close_peers = lookup::find_node(routing.clone(), bootstrap.clone(), record.owner, 16).await;

    for peer in close_peers {
        let _ = client::rpc(
            &routing,
            &peer.address.to_string(),
            Message::Store {
                key: record.owner,
                record: Record::Nat(record.clone()),
            },
        )
        .await;
    }
}

/// Looks up a NAT record by walking the DHT toward the target node ID and asking nearby
/// peers whether they have a value for that key. As soon as one of them returns a NAT
/// record, we return the record. If none of them returns a NAT record, we return None.
pub async fn resolve_nat_record(
    bootstrap: Peer,
    node_id: NodeId,
    routing: Arc<Mutex<RoutingTable>>,
) -> Option<NatRecord> {
    let close_peers = lookup::find_node(routing.clone(), bootstrap.clone(), node_id, 16).await;

    for peer in close_peers {
        if let Some(Message::Value {
            record: Some(Record::Nat(nat_record)),
        }) = client::rpc(
            &routing,
            &peer.address.to_string(),
            Message::FindValue { key: node_id },
        )
        .await
        {
            return Some(nat_record);
        }
    }

    None
}

/// Publishes a hidden service descriptor by storing it on peers that are close
/// (XOR) to the hidden service hash in the DHT.
pub async fn publish_hs_descriptor(
    bootstrap: Peer,
    record: HSRecord,
    routing: Arc<Mutex<RoutingTable>>,
) {
    let close_peers = lookup::find_node(
        routing.clone(),
        bootstrap.clone(),
        record.hidden_service_hash,
        16,
    )
    .await;

    for peer in close_peers {
        let _ = client::rpc(
            &routing,
            &peer.address.to_string(),
            Message::Store {
                key: record.hidden_service_hash,
                record: Record::HiddenService(record.clone()),
            },
        )
        .await;
    }
}

/// Resolves a hidden service descriptor by searching the DHT for the its hash and
/// checking the nearby peers until one returns a descriptor.
pub async fn resolve_hs_descriptor(
    bootstrap: Peer,
    hidden_service_hash: NodeId,
    routing: Arc<Mutex<RoutingTable>>,
) -> Option<HSRecord> {
    let close_peers =
        lookup::find_node(routing.clone(), bootstrap.clone(), hidden_service_hash, 16).await;

    for peer in close_peers {
        if let Some(Message::Value {
            record: Some(Record::HiddenService(descriptor)),
        }) = client::rpc(
            &routing,
            &peer.address.to_string(),
            Message::FindValue {
                key: hidden_service_hash,
            },
        )
        .await
        {
            return Some(descriptor);
        }
    }

    None
}
