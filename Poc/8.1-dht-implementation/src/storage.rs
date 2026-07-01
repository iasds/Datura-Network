use std::collections::HashMap;

use crate::identity::NodeId;
use crate::records::Record;


pub struct Storage {
    records: HashMap<[u8;32], Record>,
}


impl Storage {

    pub fn new() -> Self {
        Self {
            records: HashMap::new()
        }
    }


    pub fn put(
        &mut self,
        key: NodeId,
        value: Record,
    ) {
        self.records.insert(
            key,
            value
        );
    }


    pub fn get(
        &self,
        key: &NodeId,
    ) -> Option<&Record> {
        self.records.get(key)
    }

}