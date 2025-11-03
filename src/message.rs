use serde::{Deserialize, Serialize};

pub type NodeId = u32;

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Hello { from: NodeId },
    Heartbeat { from: NodeId, term: u64 },
    Election { from: NodeId },
    Ok { from: NodeId },
    Coordinator { leader: NodeId, term: u64 },

    // Leader discovery for clients
    QueryLeader,
    LeaderInfo { leader_id: Option<NodeId>, term: u64 },

    // Image encryption service
    EncryptRequest { from: NodeId, req_id: String, user: String, image_bytes: Vec<u8>, client_addr: Option<String> },
    EncryptReply {
        req_id: String,
        ok: bool,
        encrypted_image: Option<Vec<u8>>,  // Encrypted image bytes returned to client
        original_filename: Option<String>,  // Original filename for client storage
        error: Option<String>
    },
}