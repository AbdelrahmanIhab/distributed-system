use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::RwLock,
    time::timeout,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use bytes::{Bytes, BytesMut};
use anyhow::{Context, Result};

use crate::message::{Message, NodeId};

// Needed for .next() and .send()
use futures::{SinkExt, StreamExt};

/// Handles networking between cloud nodes (message sending and listening).
#[derive(Clone)]
pub struct Net {
    me: NodeId,
    peers: Arc<RwLock<HashMap<NodeId, SocketAddr>>>,
    conns: Arc<RwLock<HashMap<NodeId, Framed<TcpStream, LengthDelimitedCodec>>>>,
}

impl Net {
    /// Create a new Net manager for a node
    pub fn new(me: NodeId, peer_map: HashMap<NodeId, SocketAddr>) -> Self {
        Self {
            me,
            peers: Arc::new(RwLock::new(peer_map)),
            conns: Arc::new(RwLock::new(HashMap::new())),
        }
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
            let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
            let on_msg = on_msg.clone();

            // Spawn a task to handle messages from this connection
            tokio::spawn(async move {
                while let Some(frame_res) = framed.next().await {
                    match frame_res {
                        Ok(bytes_mut) => {
                            let slice: &[u8] = &bytes_mut;
                            match serde_json::from_slice::<Message>(slice) {
                                Ok(msg) => on_msg(addr, msg),
                                Err(e) => eprintln!("Decode error from {addr}: {e}"),
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
        let mut conns = self.conns.write().await;

        // Ensure a connection exists
        if !conns.contains_key(&to) {
            let addr = {
                let peers = self.peers.read().await;
                *peers.get(&to).context("Unknown peer ID")?
            };
            let stream = timeout(Duration::from_secs(2), TcpStream::connect(addr)).await??;
            let framed = Framed::new(stream, LengthDelimitedCodec::new());
            conns.insert(to, framed);
        }

        // Send the message (remove connection if send fails)
        if let Some(framed) = conns.get_mut(&to) {
            if let Err(e) = framed.send(Bytes::from(payload)).await {
                eprintln!("Send error to {to}: {e} — dropping connection");
                conns.remove(&to);
            }
        }

        Ok(())
    }
}
