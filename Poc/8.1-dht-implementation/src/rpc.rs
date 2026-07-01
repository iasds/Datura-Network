use serde::{Serialize, Deserialize};

use crate::{
    identity::NodeId,
    records::Record,
    routing::Peer,
};


#[derive(Serialize, Deserialize, Debug)]
pub enum Message {

    Ping,


    Pong {
        id: NodeId,
        peer: Peer,
    },

    Hello {
        peer: Peer,
    },

    HelloAck {
        peer: Peer,
    },


    FindNode {
        target: NodeId,
    },


    Nodes {
        peers: Vec<Peer>,
    },


    Store {
        key: NodeId,
        record: Record,
    },


    FindValue {
        key: NodeId,
    },


    Value {
        record: Option<Record>,
    },
}