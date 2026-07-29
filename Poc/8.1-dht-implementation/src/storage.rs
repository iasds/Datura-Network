use std::collections::HashMap;

use crate::identity::NodeId;
use crate::records::Record;

/// A local store for values that this node holds for the DHT.
pub struct Storage {
    records: HashMap<[u8; 32], Record>,
}

impl Storage {
    /// Create an empty store for the node's local DHT records.
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    /// Save a record under its key so later lookups can find it without asking the network again.
    pub fn put(&mut self, key: NodeId, value: Record) {
        self.records.insert(key, value);
    }

    /// Retrieve a record by key (when this node is the one that actually holds it).
    pub fn get(&self, key: &NodeId) -> Option<&Record> {
        self.records.get(key)
    }
}
