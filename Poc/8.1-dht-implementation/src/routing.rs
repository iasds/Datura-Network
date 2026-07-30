use crate::identity::NodeId;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, net::SocketAddr};

/// A small cap per bucket keeps the routing table from growing into a swamp of stale nodes.
pub const K: usize = 20;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Peer {
    pub id: NodeId,
    pub address: SocketAddr,
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
        if let Some(existing_position) = self
            .peers
            .iter()
            .position(|existing_peer| existing_peer.id == peer.id)
        {
            self.peers.remove(existing_position);
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
                address: std::net::SocketAddr::from(([0, 0, 0, 0], 0)),
            },
            buckets,
        }
    }

    /// Set the node's own identity once the process has a real local peer to represent.
    /// MUST be called after new()
    pub fn set_local_peer(&mut self, peer: Peer) {
        self.local_peer = peer;
        let bucket_position = bucket_index(&self.local_peer.id, &self.local_peer.id);

        self.buckets[bucket_position].insert(self.local_peer.clone());
    }

    /// Remember a newly discovered peer by placing it in the bucket that matches its distance.
    pub fn add_peer(&mut self, peer: Peer) {
        if peer.id == self.local_peer.id {
            return;
        }

        let bucket_position = bucket_index(&self.local_peer.id, &peer.id);

        self.buckets[bucket_position].insert(peer);
    }

    fn consider_bucket(
        &self,
        bucket_position: usize,
        target: &NodeId,
        result_count: usize,
        best_peers: &mut Vec<Peer>,
    ) {
        for peer in self.buckets[bucket_position].peers() {
            // avoid duplicates
            if best_peers.iter().any(|best_peer| best_peer.id == peer.id) {
                continue;
            }

            let distance = xor_distance(&peer.id, target);

            // Find insertion point.
            let insertion_index = best_peers
                .binary_search_by(|candidate_peer| {
                    xor_distance(&candidate_peer.id, target).cmp(&distance)
                })
                .unwrap_or_else(|index| index);

            if insertion_index < result_count {
                best_peers.insert(insertion_index, peer.clone());

                if best_peers.len() > result_count {
                    best_peers.pop();
                }
            } else if best_peers.len() < result_count {
                best_peers.push(peer.clone());

                best_peers.sort_by(|first_peer, second_peer| {
                    xor_distance(&first_peer.id, target).cmp(&xor_distance(&second_peer.id, target))
                });
            }
        }
    }

    /// Return the peers that look best for a lookup toward the given target.
    /// The search expands outward from the most relevant bucket until
    /// it has enough candidates (set by `result_count`).
    pub fn closest(&self, target: NodeId, result_count: usize) -> Vec<Peer> {
        let center_bucket = bucket_index(&self.local_peer.id, &target);

        let mut best_peers: Vec<Peer> = Vec::with_capacity(result_count);

        for radius in 0..256 {
            if let Some(lower_bucket) = center_bucket.checked_sub(radius) {
                self.consider_bucket(lower_bucket, &target, result_count, &mut best_peers);
            }

            if radius != 0 {
                let upper_bucket = center_bucket + radius;

                if upper_bucket < 256 {
                    self.consider_bucket(upper_bucket, &target, result_count, &mut best_peers);
                }
            }
        }

        best_peers
    }
}

/// XOR distance compares how "close" two node IDs are.
pub fn xor_distance(first_id: &NodeId, second_id: &NodeId) -> [u8; 32] {
    let mut distance = [0u8; 32];

    for byte_index in 0..32 {
        distance[byte_index] = first_id[byte_index] ^ second_id[byte_index];
    }

    distance
}

/// Map a node ID to the bucket that should hold it.
/// The bucket index reflects how far the a peer is from the local node in XOR space.
/// 0 means farthest and 255 means the same node (closest possible).
pub fn bucket_index(local: &NodeId, remote: &NodeId) -> usize {
    let distance = xor_distance(local, remote);

    for (byte_index, byte) in distance.iter().enumerate() {
        if *byte == 0 {
            continue;
        }

        let leading_zero_bits = byte.leading_zeros() as usize;

        let bit_position = byte_index * 8 + leading_zero_bits;

        return 255 - bit_position;
    }

    // Same node.
    255
}
