use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::RwLock,
    time::timeout,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use bytes::Bytes;
use anyhow::{Context, Result};
use anyhow::anyhow;

use aes_gcm::{Aes256Gcm, aead::{Aead, KeyInit}, Nonce};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::message::{Message, NodeId};

// Needed for .next() and .send()
use futures::{SinkExt, StreamExt};

/// Handles networking between cloud nodes (message sending and listening).
#[derive(Clone)]
pub struct Net {
    me: NodeId,
    peers: Arc<RwLock<HashMap<NodeId, SocketAddr>>>,
    conns: Arc<RwLock<HashMap<NodeId, Framed<TcpStream, LengthDelimitedCodec>>>>,
    // shared symmetric key (must be same across peers to communicate)
    key: Arc<Vec<u8>>,
}

impl Net {
    /// Create a new Net manager for a node
    /// Create a new Net manager for a node. `key` must be 32 bytes (AES-256 key).
    pub fn new(me: NodeId, peer_map: HashMap<NodeId, SocketAddr>, key: Vec<u8>) -> Self {
        Self {
            me,
            peers: Arc::new(RwLock::new(peer_map)),
            conns: Arc::new(RwLock::new(HashMap::new())),
            key: Arc::new(key),
        }
    }

    // Construct Aes256Gcm from stored key
    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(&self.key).expect("AES key must be 32 bytes")
    }

    /// Encrypt plaintext using AES-256-GCM. Output = 12-byte nonce || ciphertext
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = self.cipher();
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let nonce_slice = Nonce::from_slice(&nonce);
        let ct = cipher
            .encrypt(nonce_slice, plaintext)
            .map_err(|e| anyhow!(e.to_string()))?;
        let mut out = Vec::with_capacity(12 + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Decrypt data produced by `encrypt`. Expects 12-byte nonce prefix.
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(anyhow!("ciphertext too short"));
        }
        let (nonce, ct) = data.split_at(12);
        let plain = self
            .cipher()
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|e| anyhow!(e.to_string()))?;
        Ok(plain)
    }

    /// Starts a TCP listener for incoming messages and spawns a task for each connection.
    pub async fn run_listener<F>(&self, bind: SocketAddr, on_msg: F) -> Result<()>
    where
        F: Fn(SocketAddr, Message) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind(bind)
            .await
            .with_context(|| format!("bind failed on {}", bind))?;
        println!("[{}] Listening on {}", self.me, bind);

        let on_msg = Arc::new(on_msg);

        loop {
            let (stream, addr) = listener.accept().await?;
            // Set max frame size to 100MB to support large images
            let mut codec = LengthDelimitedCodec::new();
            codec.set_max_frame_length(100 * 1024 * 1024);
            let mut framed = Framed::new(stream, codec);
            let on_msg = on_msg.clone();
            let net_clone = self.clone();

            // Spawn a task to handle messages from this connection
            tokio::spawn(async move {
                while let Some(frame_res) = framed.next().await {
                    match frame_res {
                        Ok(bytes_mut) => {
                            let slice: &[u8] = &bytes_mut;
                            // Decrypt frame first
                            match net_clone.decrypt(slice) {
                                Ok(plain) => match serde_json::from_slice::<Message>(&plain) {
                                    Ok(msg) => on_msg(addr, msg),
                                    Err(e) => eprintln!("Decode error from {addr}: {e}"),
                                },
                                Err(e) => eprintln!("Decrypt error from {addr}: {e}"),
                            }
                        }
                        Err(e) => {
                            eprintln!("Read error from {addr}: {e}");
                            break;
                        }
                    }
                }
            });
        }
    }

    /// Sends a serialized message to another node.
    pub async fn send(&self, to: NodeId, msg: &Message) -> Result<()> {
        let payload = serde_json::to_vec(msg)?;
        let encrypted_size = payload.len();
        let payload = self.encrypt(&payload)?;
        let final_size = payload.len();

        let mut conns = self.conns.write().await;

        // Ensure a connection exists
        if !conns.contains_key(&to) {
            let addr = {
                let peers = self.peers.read().await;
                *peers.get(&to).context("Unknown peer ID")?
            };
            println!("[Net] Connecting to node {} at {}", to, addr);
            let stream = timeout(Duration::from_secs(5), TcpStream::connect(addr)).await??;
            // Set max frame size to 100MB to support large images
            let mut codec = LengthDelimitedCodec::new();
            codec.set_max_frame_length(100 * 1024 * 1024);
            let framed = Framed::new(stream, codec);
            conns.insert(to, framed);
            println!("[Net] Connected to node {}", to);
        }

        // Send the message (remove connection if send fails and return error)
        if let Some(framed) = conns.get_mut(&to) {
            println!("[Net] Sending message to node {} (payload: {} bytes, encrypted: {} bytes)", to, encrypted_size, final_size);
            match framed.send(Bytes::from(payload)).await {
                Ok(()) => {
                    println!("[Net] Successfully sent message to node {}", to);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("[Net] Send error to {}: {} — dropping connection", to, e);
                    conns.remove(&to);
                    Err(anyhow!("Failed to send message to node {}: {}", to, e))
                }
            }
        } else {
            Err(anyhow!("No connection to node {}", to))
        }
    }

    /// Temporarily add a peer address for sending a message
    pub async fn add_temp_peer(&self, node_id: NodeId, addr: SocketAddr) {
        let mut peers = self.peers.write().await;
        peers.insert(node_id, addr);
    }

    /// Remove a temporary peer
    pub async fn remove_temp_peer(&self, node_id: NodeId) {
        let mut peers = self.peers.write().await;
        peers.remove(&node_id);
    }
}
