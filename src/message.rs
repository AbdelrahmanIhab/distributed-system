use serde::{Deserialize, Serialize};

pub type NodeId = u32;

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Hello { from: NodeId },
    Heartbeat { from: NodeId, term: u64 },
    Election { from: NodeId },
    Ok { from: NodeId },
    Coordinator { leader: NodeId, term: u64 },

    // app-level example
    EncryptRequest { req_id: String, user: String, image_bytes: Vec<u8> },
    EncryptReply { req_id: String, ok: bool, payload: Option<Vec<u8>>, error: Option<String> },
}