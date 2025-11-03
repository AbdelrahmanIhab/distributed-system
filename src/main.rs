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

    println!("[Node {}] Starting on {}", me, bind);
    println!("[Node {}] Peers: {:?}", me, peers);

    let key = vec![0u8; 32];
    let net = Net::new(me, peers.clone(), key);
    let peers = Arc::new(peers);

    // Client request tracking
    let client_requests: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(HashMap::new()));

    // Round-robin worker selection
    let next_worker_idx: Arc<RwLock<usize>> = Arc::new(RwLock::new(0));

    // Leader election state
    let leader: Arc<RwLock<Option<NodeId>>> = Arc::new(RwLock::new(None));
    let participating = Arc::new(RwLock::new(false));
    let last_heartbeat: Arc<RwLock<Option<Instant>>> = Arc::new(RwLock::new(None));
    let ok_received = Arc::new(AtomicBool::new(false));
    let heartbeat_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>> = Arc::new(RwLock::new(None));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(SocketAddr, Message)>();

    // Start listener
    let net_listener = net.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let on_msg = move |addr: SocketAddr, msg: Message| {
            let _ = tx_clone.send((addr, msg));
        };
        if let Err(e) = net_listener.run_listener(bind, on_msg).await {
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
        while let Some((_addr, msg)) = rx.recv().await {
            match msg {
                // ELECTION MESSAGES
                Message::Election { from } => {
                    if me > from {
                        println!("[Node {}] Election from {}, sending OK", me, from);
                        net_proc.send(from, &Message::Ok { from: me }).await.ok();
                        start_election(me, net_proc.clone(), peers_proc.clone(), participating_proc.clone(),
                                     ok_received_proc.clone(), leader_proc.clone(), heartbeat_handle_proc.clone()).await;
                    }
                }
                Message::Ok { from: _ } => {
                    ok_received_proc.store(true, Ordering::SeqCst);
                }
                Message::Coordinator { leader: ldr, term: _ } => {
                    println!("[Node {}] Node {} is now LEADER", me, ldr);
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
                Message::EncryptRequest { from, req_id, user: _, image_bytes, client_addr } => {
                    let is_leader = leader_proc.read().await.map(|l| l == me).unwrap_or(false);

                    if is_leader && from == 0 {
                        // LEADER GOT REQUEST FROM CLIENT
                        println!("[Node {}] LEADER: Got request {} from client ({} bytes)", me, req_id, image_bytes.len());

                        // Store client address
                        if let Some(ref addr_str) = client_addr {
                            client_requests_proc.write().await.insert(req_id.clone(), addr_str.clone());
                        }

                        // Pick worker using round-robin
                        let worker_list: Vec<NodeId> = peers_proc.keys().copied().collect();
                        let worker = if !worker_list.is_empty() {
                            let mut idx = next_worker_idx_proc.write().await;
                            let selected = worker_list[*idx % worker_list.len()];
                            *idx = (*idx + 1) % worker_list.len();
                            selected
                        } else {
                            me
                        };

                        println!("[Node {}] LEADER: Forwarding {} to worker {}", me, req_id, worker);

                        if worker == me {
                            // Leader processes itself
                            let (ext, _) = detect_image_format(&image_bytes);
                            match net_proc.encrypt(&image_bytes) {
                                Ok(encrypted) => {
                                    println!("[Node {}] LEADER: Encrypted {} locally", me, req_id);
                                    let reply = Message::EncryptReply {
                                        req_id: req_id.clone(),
                                        ok: true,
                                        encrypted_image: Some(encrypted),
                                        original_filename: Some(format!("image.{}", ext)),
                                        error: None,
                                    };

                                    // Send to client
                                    if let Some(addr_str) = client_addr {
                                        if let Ok(addr) = addr_str.parse::<SocketAddr>() {
                                            let n = net_proc.clone();
                                            tokio::spawn(async move {
                                                n.add_temp_peer(0, addr).await;
                                                n.send(0, &reply).await.ok();
                                                n.remove_temp_peer(0).await;
                                            });
                                        }
                                    }
                                }
                                Err(e) => eprintln!("[Node {}] Encryption error: {}", me, e),
                            }
                        } else {
                            // Forward to worker
                            let msg = Message::EncryptRequest {
                                from: me,
                                req_id,
                                user: String::new(),
                                image_bytes,
                                client_addr,
                            };
                            net_proc.send(worker, &msg).await.ok();
                        }
                    } else {
                        // WORKER PROCESSES REQUEST
                        println!("[Node {}] WORKER: Processing request {}", me, req_id);
                        let (ext, _) = detect_image_format(&image_bytes);

                        match net_proc.encrypt(&image_bytes) {
                            Ok(encrypted) => {
                                println!("[Node {}] WORKER: Encrypted {} ({} bytes)", me, req_id, encrypted.len());

                                // ALWAYS send back to leader
                                let reply = Message::EncryptReply {
                                    req_id: req_id.clone(),
                                    ok: true,
                                    encrypted_image: Some(encrypted),
                                    original_filename: Some(format!("image.{}", ext)),
                                    error: None,
                                };

                                println!("[Node {}] WORKER: Sending {} back to leader", me, req_id);
                                net_proc.send(from, &reply).await.ok();
                            }
                            Err(e) => {
                                eprintln!("[Node {}] WORKER: Encryption failed for {}: {}", me, req_id, e);
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
                        println!("[Node {}] LEADER: Got encrypted reply for {}", me, req_id);

                        if let Some(encrypted_bytes) = encrypted_image {
                            let client_addr_opt = client_requests_proc.read().await.get(&req_id).cloned();

                            if let Some(client_addr_str) = client_addr_opt {
                                println!("[Node {}] LEADER: Forwarding {} to client", me, req_id);

                                let reply = Message::EncryptReply {
                                    req_id: req_id.clone(),
                                    ok: true,
                                    encrypted_image: Some(encrypted_bytes),
                                    original_filename,
                                    error: None,
                                };

                                if let Ok(client_sock_addr) = client_addr_str.parse::<SocketAddr>() {
                                    let n = net_proc.clone();
                                    tokio::spawn(async move {
                                        n.add_temp_peer(0, client_sock_addr).await;
                                        match n.send(0, &reply).await {
                                            Ok(()) => println!("[Node {}] LEADER: Sent to client", me),
                                            Err(e) => eprintln!("[Node {}] LEADER: Failed to send: {}", me, e),
                                        }
                                        n.remove_temp_peer(0).await;
                                    });
                                }

                                client_requests_proc.write().await.remove(&req_id);
                            }
                        }
                    } else {
                        eprintln!("[Node {}] Got error reply for {}: {:?}", me, req_id, error);
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
                    println!("[Node {}] Leader timeout, starting election", me);
                    start_election(me, net.clone(), peers.clone(), participating.clone(),
                                 ok_received.clone(), leader.clone(), heartbeat_handle.clone()).await;
                }
            }
        } else {
            println!("[Node {}] No leader, starting election", me);
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

    println!("[Node {}] Starting election", me);
    ok_received.store(false, Ordering::SeqCst);

    let higher: Vec<NodeId> = peers.keys().filter(|&&id| id > me).cloned().collect();

    if higher.is_empty() {
        become_leader(me, net, peers, leader, heartbeat_handle).await;
        *participating.write().await = false;
        return;
    }

    for id in &higher {
        net.send(*id, &Message::Election { from: me }).await.ok();
    }

    sleep(Duration::from_secs(2)).await;

    if ok_received.load(Ordering::SeqCst) {
        println!("[Node {}] Higher node responded, waiting...", me);
        sleep(Duration::from_secs(2)).await;
    } else {
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
    println!("[Node {}] I am the LEADER", me);
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
                sleep(Duration::from_millis(500)).await;
            }
        }
    });

    *heartbeat_handle.write().await = Some(h);
}