use crate::identity::NodeId;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    net::SocketAddr,
};

/// A small cap per bucket keeps the routing table from growing into a swamp of stale nodes.
pub const K: usize = 20;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Peer {
    pub id: NodeId,
    pub addr: SocketAddr,
}

/// A bucket holds peers that are roughly the same distance away from a node.
/// That makes it practical to find nodes that are likely to be useful for a given lookup.
pub struct KBucket {
    peers: VecDeque<Peer>,
}

impl KBucket {
    pub fn new() -> Self {
        Self {
            peers: VecDeque::new(),
        }
    }

    pub fn insert(&mut self, peer: Peer) {

        if let Some(pos) =
            self.peers.iter().position(|p| p.id == peer.id)
        {
            self.peers.remove(pos);
            self.peers.push_back(peer);
            return;
        }

        if self.peers.len() < K {
            self.peers.push_back(peer);
        }
    }

    pub fn peers(&self) -> impl Iterator<Item = &Peer> {
        self.peers.iter()
    }
}

/// The routing table is the node's local map of the network.
pub struct RoutingTable {

    local_peer: Peer,

    buckets: Vec<KBucket>,
}

impl RoutingTable {

    pub fn new() -> Self {

        let mut buckets = Vec::with_capacity(256);

        for _ in 0..256 {
            buckets.push(KBucket::new());
        }

        Self {
            local_peer: Peer {
                id: [0u8; 32],
                addr: std::net::SocketAddr::from(([0, 0, 0, 0], 0)),
            },
            buckets,
        }
    }

    /// Set the node's own identity once the process has a real local peer to represent.
    /// MUST be called after new()
    pub fn set_local_peer(
        &mut self,
        peer: Peer,
    ) {
        self.local_peer = peer;
        let bucket =
            bucket_index(
                &self.local_peer.id,
                &self.local_peer.id,
            );

        self.buckets[bucket]
            .insert(self.local_peer.clone());
    }

    /// Remember a newly discovered peer by placing it in the bucket that matches its distance.
    pub fn add_peer(
        &mut self,
        peer: Peer,
    ) {

        if peer.id == self.local_peer.id {
            return;
        }

        let bucket =
            bucket_index(
                &self.local_peer.id,
                &peer.id,
            );

        self.buckets[bucket]
            .insert(peer);
    }

    fn consider_bucket(
        &self,
        bucket_index: usize,
        target: &NodeId,
        count: usize,
        best: &mut Vec<Peer>,
    ) {

        for peer in self.buckets[bucket_index].peers() {

            // avoid duplicates
            if best.iter().any(|p| p.id == peer.id) {
                continue;
            }

            let distance =
                xor_distance(&peer.id, target);

            // Find insertion point.
            let pos = best
                .binary_search_by(|p| {
                    xor_distance(&p.id, target)
                        .cmp(&distance)
                })
                .unwrap_or_else(|e| e);

            if pos < count {

                best.insert(pos, peer.clone());

                if best.len() > count {
                    best.pop();
                }

            } else if best.len() < count {

                best.push(peer.clone());

                best.sort_by(|a, b| {
                    xor_distance(&a.id, target)
                        .cmp(&xor_distance(&b.id, target))
                });
            }
        }
    }

    /// Return the peers that look best for a lookup toward the given target.
    /// The search expands outward from the most relevant bucket until
    /// it has enough candidates (set by `count`).
    pub fn closest(
        &self,
        target: NodeId,
        count: usize,
    ) -> Vec<Peer> {

        let center =
            bucket_index(
                &self.local_peer.id,
                &target,
            );

        let mut best: Vec<Peer> = Vec::with_capacity(count);

        for radius in 0..256 {


            if let Some(index) = center.checked_sub(radius) {
                self.consider_bucket(
                    index,
                    &target,
                    count,
                    &mut best,
                );
            }

            if radius != 0 {

                let index = center + radius;

                if index < 256 {
                    self.consider_bucket(
                        index,
                        &target,
                        count,
                        &mut best,
                    );
                }
            }
        }

        best
    }

}

/// XOR distance compares how "close" two node IDs are.
pub fn xor_distance(
    a: &NodeId,
    b: &NodeId,
) -> [u8; 32] {

    let mut out = [0u8; 32];

    for i in 0..32 {
        out[i] = a[i] ^ b[i];
    }

    out
}

/// Map a node ID to the bucket that should hold it.
/// The bucket index reflects how far the a peer is from the local node in XOR space.
/// 0 means farthest and 255 means the same node (closest possible).
pub fn bucket_index(
    local: &NodeId,
    remote: &NodeId,
) -> usize {

    let distance =
        xor_distance(local, remote);

    for (byte_index, byte)
        in distance.iter().enumerate()
    {

        if *byte == 0 {
            continue;
        }

        let leading =
            byte.leading_zeros() as usize;

        let bit =
            byte_index * 8 + leading;

        return 255 - bit;
    }

    // Same node.
    255
}