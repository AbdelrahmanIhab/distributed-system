mod message;
mod net;
mod config;

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Instant};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{sleep, Duration};
use tokio::sync::RwLock;
use message::{Message, NodeId};
use net::Net;
use crate::config::Config;

/// Detects image format from magic bytes
fn detect_image_format(bytes: &[u8]) -> (&'static str, &'static str) {
    if bytes.len() >= 8 && bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        ("png", "image/png")
    } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        ("jpg", "image/jpeg")
    } else if bytes.len() >= 2 && bytes[0] == 0x42 && bytes[1] == 0x4D {
        ("bmp", "image/bmp")
    } else if bytes.len() >= 6 && &bytes[0..6] == b"GIF89a" || &bytes[0..6] == b"GIF87a" {
        ("gif", "image/gif")
    } else {
        ("bin", "application/octet-stream")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let me: NodeId = std::env::var("NODE_ID").unwrap_or("1".into()).parse()?;

    let config_path = std::env::var("CONFIG_FILE").ok();
    let config = Config::load(config_path.as_deref())?;
    let peers = config.to_peer_map()?;

    let bind: SocketAddr = peers.get(&me)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("Node {} not found in peer configuration", me))?;

    println!("═══════════════════════════════════════");
    println!("  Node {} Starting", me);
    println!("  Listening on: {}", bind);
    println!("═══════════════════════════════════════");

    let key = vec![0u8; 32];
    let net = Net::new(me, peers.clone(), key.clone());
    let peers = Arc::new(peers);

    // Client request tracking - maps req_id to reply sender
    type ReplySender = tokio::sync::mpsc::UnboundedSender<Message>;
    let client_requests: Arc<RwLock<HashMap<String, ReplySender>>> = Arc::new(RwLock::new(HashMap::new()));

    // Round-robin worker selection
    let next_worker_idx: Arc<RwLock<usize>> = Arc::new(RwLock::new(0));

    // Leader election state
    let leader: Arc<RwLock<Option<NodeId>>> = Arc::new(RwLock::new(None));
    let participating = Arc::new(RwLock::new(false));
    let last_heartbeat: Arc<RwLock<Option<Instant>>> = Arc::new(RwLock::new(None));
    let ok_received = Arc::new(AtomicBool::new(false));
    let heartbeat_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>> = Arc::new(RwLock::new(None));

    type MessageWithSender = (SocketAddr, Message, Option<ReplySender>);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<MessageWithSender>();

    // Start listener with bidirectional support
    let net_listener = net.clone();
    let tx_clone = tx.clone();
    let key_clone = key.clone();
    tokio::spawn(async move {
        if let Err(e) = net_listener.run_listener_bidirectional(bind, key_clone, tx_clone).await {
            eprintln!("[Node {}] listener error: {}", me, e);
        }
    });

    // Message processor
    let net_proc = net.clone();
    let leader_proc = leader.clone();
    let participating_proc = participating.clone();
    let last_heartbeat_proc = last_heartbeat.clone();
    let ok_received_proc = ok_received.clone();
    let heartbeat_handle_proc = heartbeat_handle.clone();
    let peers_proc = peers.clone();
    let client_requests_proc = client_requests.clone();
    let next_worker_idx_proc = next_worker_idx.clone();

    tokio::spawn(async move {
        while let Some((_addr, msg, reply_tx_opt)) = rx.recv().await {
            match msg {
                // ELECTION MESSAGES
                Message::Election { from } => {
                    if me > from {
                        println!("[Election] Node {} → Node {}: OK", me, from);
                        net_proc.send(from, &Message::Ok { from: me }).await.ok();
                        start_election(me, net_proc.clone(), peers_proc.clone(), participating_proc.clone(),
                                     ok_received_proc.clone(), leader_proc.clone(), heartbeat_handle_proc.clone()).await;
                    }
                }
                Message::Ok { from: _ } => {
                    ok_received_proc.store(true, Ordering::SeqCst);
                }
                Message::Coordinator { leader: ldr, term: _ } => {
                    println!("✓ Node {} elected as LEADER", ldr);
                    *leader_proc.write().await = Some(ldr);
                    *participating_proc.write().await = false;
                    *last_heartbeat_proc.write().await = Some(Instant::now());
                    if ldr != me {
                        if let Some(h) = heartbeat_handle_proc.write().await.take() {
                            h.abort();
                        }
                    }
                }
                Message::Heartbeat { from, term: _ } => {
                    *leader_proc.write().await = Some(from);
                    *last_heartbeat_proc.write().await = Some(Instant::now());
                }

                // ENCRYPTION REQUEST
                Message::EncryptRequest { from, req_id, user: _, image_bytes, client_addr: _ } => {
                    let is_leader = leader_proc.read().await.map(|l| l == me).unwrap_or(false);

                    if is_leader && from == 0 {
                        // LEADER GOT REQUEST FROM CLIENT
                        println!("[Leader] Request {} from client ({} bytes)", req_id, image_bytes.len());

                        // Store reply sender to respond to client
                        if let Some(tx) = reply_tx_opt {
                            client_requests_proc.write().await.insert(req_id.clone(), tx);
                        }

                        // Pick worker using round-robin (exclude leader itself)
                        let worker_list: Vec<NodeId> = peers_proc.keys()
                            .filter(|&&id| id != me)
                            .copied()
                            .collect();

                        let worker = if !worker_list.is_empty() {
                            let mut idx = next_worker_idx_proc.write().await;
                            let selected = worker_list[*idx % worker_list.len()];
                            *idx += 1;
                            println!("[Load Balancer] Selected worker: Node {}", selected);
                            selected
                        } else {
                            println!("[Load Balancer] No workers available, processing locally");
                            me
                        };

                        if worker == me {
                            // Leader processes itself
                            let (ext, _) = detect_image_format(&image_bytes);
                            match net_proc.encrypt(&image_bytes) {
                                Ok(encrypted) => {
                                    println!("[Worker] Node {} encrypted {} ({} bytes)", me, req_id, encrypted.len());
                                    let reply = Message::EncryptReply {
                                        req_id: req_id.clone(),
                                        ok: true,
                                        encrypted_image: Some(encrypted),
                                        original_filename: Some(format!("image.{}", ext)),
                                        error: None,
                                    };

                                    // Send reply back to client via stored channel
                                    if let Some(tx) = client_requests_proc.write().await.remove(&req_id) {
                                        let _ = tx.send(reply);
                                    }
                                }
                                Err(e) => eprintln!("[Error] Encryption failed: {}", e),
                            }
                        } else {
                            // Forward to worker
                            let msg = Message::EncryptRequest {
                                from: me,
                                req_id,
                                user: String::new(),
                                image_bytes,
                                client_addr: None,
                            };
                            net_proc.send(worker, &msg).await.ok();
                        }
                    } else if from != 0 {
                        // WORKER PROCESSES REQUEST
                        println!("[Worker] Node {} processing request {}", me, req_id);
                        let (ext, _) = detect_image_format(&image_bytes);

                        match net_proc.encrypt(&image_bytes) {
                            Ok(encrypted) => {
                                println!("[Worker] Node {} encrypted {} ({} bytes)", me, req_id, encrypted.len());

                                // Send back to leader
                                let reply = Message::EncryptReply {
                                    req_id: req_id.clone(),
                                    ok: true,
                                    encrypted_image: Some(encrypted),
                                    original_filename: Some(format!("image.{}", ext)),
                                    error: None,
                                };

                                net_proc.send(from, &reply).await.ok();
                            }
                            Err(e) => {
                                eprintln!("[Error] Worker encryption failed: {}", e);
                                let reply = Message::EncryptReply {
                                    req_id,
                                    ok: false,
                                    encrypted_image: None,
                                    original_filename: None,
                                    error: Some(format!("{}", e)),
                                };
                                net_proc.send(from, &reply).await.ok();
                            }
                        }
                    }
                }

                // ENCRYPTION REPLY (worker → leader → client)
                Message::EncryptReply { req_id, ok, encrypted_image, original_filename, error } => {
                    if ok {
                        println!("[Leader] Received encrypted reply for {}", req_id);

                        if let Some(_encrypted_bytes) = encrypted_image.as_ref() {
                            // Forward to client via stored channel
                            if let Some(tx) = client_requests_proc.write().await.remove(&req_id) {
                                let reply = Message::EncryptReply {
                                    req_id: req_id.clone(),
                                    ok: true,
                                    encrypted_image,
                                    original_filename,
                                    error: None,
                                };
                                if tx.send(reply).is_ok() {
                                    println!("[Leader] Forwarded {} to client", req_id);
                                } else {
                                    eprintln!("[Error] Client channel closed for {}", req_id);
                                }
                            }
                        }
                    } else {
                        eprintln!("[Error] Request {} failed: {:?}", req_id, error);
                        client_requests_proc.write().await.remove(&req_id);
                    }
                }

                _ => {}
            }
        }
    });

    // Initial election
    sleep(Duration::from_millis(200 + (me as u64 * 50))).await;
    start_election(me, net.clone(), peers.clone(), participating.clone(), ok_received.clone(),
                   leader.clone(), heartbeat_handle.clone()).await;

    // Monitor heartbeats
    loop {
        sleep(Duration::from_secs(1)).await;

        let leader_opt = *leader.read().await;
        let last = *last_heartbeat.read().await;
        let participating_now = *participating.read().await;

        if participating_now {
            continue;
        }

        if let Some(ldr) = leader_opt {
            if ldr != me {
                let stale = last.map(|t| Instant::now().duration_since(t) > Duration::from_secs(3)).unwrap_or(true);
                if stale {
                    println!("[Election] Leader timeout, starting new election");
                    start_election(me, net.clone(), peers.clone(), participating.clone(),
                                 ok_received.clone(), leader.clone(), heartbeat_handle.clone()).await;
                }
            }
        } else {
            start_election(me, net.clone(), peers.clone(), participating.clone(),
                         ok_received.clone(), leader.clone(), heartbeat_handle.clone()).await;
        }
    }
}

async fn start_election(
    me: NodeId,
    net: Net,
    peers: Arc<HashMap<NodeId, SocketAddr>>,
    participating: Arc<RwLock<bool>>,
    ok_received: Arc<AtomicBool>,
    leader: Arc<RwLock<Option<NodeId>>>,
    heartbeat_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
) {
    {
        let mut p = participating.write().await;
        if *p {
            return;
        }
        *p = true;
    }

    println!("[Election] Node {} starting election (Bully algorithm)", me);
    ok_received.store(false, Ordering::SeqCst);

    let higher: Vec<NodeId> = peers.keys().filter(|&&id| id > me).cloned().collect();

    if higher.is_empty() {
        println!("[Election] Node {} has highest ID, becoming leader", me);
        become_leader(me, net, peers, leader, heartbeat_handle).await;
        *participating.write().await = false;
        return;
    }

    println!("[Election] Node {} sending Election to higher nodes: {:?}", me, higher);
    for id in &higher {
        net.send(*id, &Message::Election { from: me }).await.ok();
    }

    sleep(Duration::from_secs(2)).await;

    if ok_received.load(Ordering::SeqCst) {
        println!("[Election] Node {} received OK, waiting for coordinator", me);
        sleep(Duration::from_secs(2)).await;
    } else {
        println!("[Election] Node {} received no OK, becoming leader", me);
        become_leader(me, net, peers, leader, heartbeat_handle).await;
    }

    *participating.write().await = false;
}

async fn become_leader(
    me: NodeId,
    net: Net,
    peers: Arc<HashMap<NodeId, SocketAddr>>,
    leader: Arc<RwLock<Option<NodeId>>>,
    heartbeat_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
) {
    println!("═══════════════════════════════════════");
    println!("  Node {} is now LEADER", me);
    println!("═══════════════════════════════════════");
    *leader.write().await = Some(me);

    for (&id, _) in peers.iter() {
        if id == me { continue; }
        net.send(id, &Message::Coordinator { leader: me, term: 1 }).await.ok();
    }

    let h = tokio::spawn({
        let net = net.clone();
        let peers = peers.clone();
        async move {
            loop {
                for (&id, _) in peers.iter() {
                    if id == me { continue; }
                    net.send(id, &Message::Heartbeat { from: me, term: 1 }).await.ok();
                }
                sleep(Duration::from_millis(1000)).await;
            }
        }
    });

    *heartbeat_handle.write().await = Some(h);
}