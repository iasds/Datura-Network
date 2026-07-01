use crate::identity::NodeId;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    net::SocketAddr,
};

/// Maximum peers per bucket.
pub const K: usize = 20;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Peer {
    pub id: NodeId,
    pub addr: SocketAddr,
}

pub struct KBucket {
    peers: VecDeque<Peer>,
}

impl KBucket {
    pub fn new() -> Self {
        Self {
            peers: VecDeque::new(),
        }
    }

    /// Insert using simple LRU.
    ///
    /// Existing peer -> move to back.
    /// New peer -> append if bucket has room.
    /// Full bucket -> ignore (PoC simplification).
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

pub struct RoutingTable {

    /// Local node ID.
    local_peer: Peer,

    /// 256 Kademlia buckets. Make it 32 for PoC?
    buckets: Vec<KBucket>,
}

impl RoutingTable {

    /// Keeps compatibility with your current code.
    ///
    /// If you don't know the local ID yet,
    /// initialize to all zeros.
    ///
    /// Later call set_local_id().
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

    /// Call once after Identity::new().
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

    /// Keeps compatibility.
    pub fn all(&self) -> Vec<Peer> {

        let mut peers: Vec<Peer> = Vec::new();

        for bucket in &self.buckets {

            for peer in bucket.peers() {
                peers.push(peer.clone());
            }
        }

        peers
    }

    /// Keeps compatibility.
    pub fn closest(
        &self,
        target: NodeId,
        count: usize,
    ) -> Vec<Peer> {

        let mut peers: Vec<Peer> =
            self.all();

        peers.sort_by(|a, b| {

            xor_distance(
                &a.id,
                &target,
            )
            .cmp(
                &xor_distance(
                    &b.id,
                    &target,
                )
            )
        });

        peers.truncate(count);

        peers
    }

    /// Used by iterative FIND_NODE.
    ///
    /// Returns the nearest peer that has
    /// not already been queried.
    pub fn nearest_unqueried(
        &self,
        target: NodeId,
        queried: &std::collections::HashSet<NodeId>,
    ) -> Option<Peer> {

        let mut peers = self.all();

        peers.retain(|p| {
            !queried.contains(&p.id)
        });

        peers.sort_by(|a, b| {

            xor_distance(
                &a.id,
                &target,
            )
            .cmp(
                &xor_distance(
                    &b.id,
                    &target,
                )
            )
        });

        peers.into_iter().next()
    }
}

/// XOR distance.
///
/// Smaller == closer.
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

/// Determine which bucket a peer belongs in.
///
/// bucket 255 = nearest
///
/// bucket 0 = farthest
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