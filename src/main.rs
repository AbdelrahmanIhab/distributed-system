mod message;
mod net;
mod balancer;
mod config;

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Instant, time::{SystemTime, UNIX_EPOCH}};
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::time::{sleep, Duration};
use tokio::sync::RwLock;

use message::{Message, NodeId};
use net::Net;
use crate::balancer::LeastLoad;
use crate::config::Config;
use rand::RngCore;
use rand::rngs::OsRng;
use hex;
use std::io::BufRead;

/// Simple Bully algorithm implementation (demo)
/// - On startup each node will attempt an election after a short jitter.
/// - Higher ID wins. Nodes respond with `Ok` to Election messages from lower IDs.
/// - If a node doesn't hear a Coordinator after timeout it becomes coordinator and
///   announces using `Coordinator` and begins sending periodic `Heartbeat` messages.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Read node ID from environment
    let me: NodeId = std::env::var("NODE_ID").unwrap_or("1".into()).parse()?;

    // Load peer configuration (from file, env, or default localhost)
    let config_path = std::env::var("CONFIG_FILE").ok();
    let config = Config::load(config_path.as_deref())?;
    let peers = config.to_peer_map()?;
    let shared_dir = Arc::new(config.shared_dir.clone());

    println!("[Node {}] Shared directory: {}", me, config.shared_dir);

    // Determine bind address: use explicit BIND env var, or lookup my address from peers
    let bind: SocketAddr = match std::env::var("BIND") {
        Ok(addr_str) => addr_str.parse()?,
        Err(_) => {
            // Look up my address from the peer config
            peers.get(&me)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("Node {} not found in peer configuration", me))?
        }
    };

    println!("[Node {}] Starting on {}", me, bind);
    println!("[Node {}] Peer configuration: {:?}", me, peers);

    // For demo purposes use a fixed 32-byte key so peers can encrypt/decrypt.
    // In a real deployment this should come from a secure shared config or env var.
    let key = vec![0u8; 32];
    let net = Net::new(me, peers.clone(), key);
    // keep a shared, cheap-to-clone handle for closures/tasks
    let peers = Arc::new(peers);
    // least-load balancer for choosing the peer with minimum load
    let balancer = Arc::new(LeastLoad::new());

    // Shared state
    let leader: Arc<RwLock<Option<NodeId>>> = Arc::new(RwLock::new(None));
    let participating = Arc::new(RwLock::new(false));
    let last_heartbeat: Arc<RwLock<Option<Instant>>> = Arc::new(RwLock::new(None));
    let ok_received = Arc::new(AtomicBool::new(false));
    let heartbeat_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>> = Arc::new(RwLock::new(None));

    // We'll forward incoming messages to a channel so the listener callback remains a simple Fn
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(SocketAddr, Message)>();

    let net_listener = net.clone();
    let tx_clone = tx.clone();
    // simple on_msg bridge: forward frames into the channel (this closure is a plain Fn)
    let on_msg = move |addr: SocketAddr, msg: Message| {
        let _ = tx_clone.send((addr, msg));
    };

    // start listener
    tokio::spawn(async move {
        if let Err(e) = net_listener.run_listener(bind, on_msg).await {
            eprintln!("[Node {}] listener error: {}", me, e);
        }
    });

    // processing task: handle messages from the channel and run election logic
    let net_proc = net.clone();
    let leader_proc = leader.clone();
    let participating_proc = participating.clone();
    let last_heartbeat_proc = last_heartbeat.clone();
    let ok_received_proc = ok_received.clone();
    let heartbeat_handle_proc = heartbeat_handle.clone();
    let peers_proc = peers.clone();
    let shared_dir_proc = shared_dir.clone();
    let balancer_proc = balancer.clone();

    tokio::spawn(async move {
        while let Some((addr, msg)) = rx.recv().await {
            match msg {
                Message::Hello { from } => {
                    println!("[Node {}] Hello from {}", me, from);
                    net_proc.send(from, &Message::Ok { from: me }).await.ok();
                }
                Message::Election { from } => {
                    println!("[Node {}] Election from {}", me, from);
                    if me > from {
                        net_proc.send(from, &Message::Ok { from: me }).await.ok();
                        start_election(
                            me,
                            net_proc.clone(),
                            peers_proc.clone(),
                            participating_proc.clone(),
                            ok_received_proc.clone(),
                            leader_proc.clone(),
                            heartbeat_handle_proc.clone(),
                        )
                        .await;
                    }
                }
                Message::Ok { from } => {
                    println!("[Node {}] Ok from {}", me, from);
                    ok_received_proc.store(true, Ordering::SeqCst);
                }
                Message::Coordinator { leader: ldr, term: _ } => {
                    println!("[Node {}] Coordinator announced: {}", me, ldr);
                    {
                        let mut g = leader_proc.write().await;
                        *g = Some(ldr);
                    }
                    {
                        let mut p = participating_proc.write().await;
                        *p = false;
                    }
                    {
                        let mut last = last_heartbeat_proc.write().await;
                        *last = Some(Instant::now());
                    }
                    if ldr != me {
                        if let Some(h) = heartbeat_handle_proc.write().await.take() {
                            h.abort();
                        }
                    }
                }
                Message::Heartbeat { from, term: _ } => {
                    {
                        let mut g = leader_proc.write().await;
                        *g = Some(from);
                    }
                    {
                        let mut last = last_heartbeat_proc.write().await;
                        *last = Some(Instant::now());
                    }
                }
                Message::EncryptRequest { from, req_id, user, image_bytes } => {
                    println!("[Node {}] ✓ RECEIVED EncryptRequest {} from {} user {} ({} bytes)", me, req_id, if from == 0 { "client" } else { &format!("node {}", from) }, user, image_bytes.len());

                    // Check if this node is the leader
                    let is_leader = {
                        let leader_opt = leader_proc.read().await;
                        leader_opt.map(|l| l == me).unwrap_or(false)
                    };

                    if is_leader && from == 0 {
                        // LEADER RECEIVES REQUEST FROM CLIENT
                        // Use load balancer to pick the least-loaded node to handle this request
                        let target_node = balancer_proc.pick(&*peers_proc, me);

                        if let Some(target) = target_node {
                            // Record the request assignment
                            balancer_proc.record_request(target);

                            if target == me {
                                // Leader selected itself as the least-loaded node - handle locally
                                println!("[Node {}] LEADER: Handling request {} locally (least loaded)", me, req_id);

                                let dir = shared_dir_proc.as_ref().clone();
                            let addr_s = addr.to_string().replace(':', "_");
                            let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                            let ext = if image_bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
                                "png"
                            } else if image_bytes.len() > 2 && image_bytes[0] == 0xFF && image_bytes[1] == 0xD8 {
                                "jpg"
                            } else {
                                "bin"
                            };
                            let fname = format!("{}_{}_{}.{}", req_id, addr_s, ts, ext);
                            let path = std::path::Path::new(&dir).join(&fname);
                            let path_for_closure = path.clone();
                            let path_for_log = path.clone();
                            let bytes_clone = image_bytes.clone();
                            let dir_clone = dir.clone();
                            let me_clone = me;

                            tokio::spawn(async move {
                                let res = tokio::task::spawn_blocking(move || {
                                    std::fs::create_dir_all(&dir_clone)?;
                                    std::fs::write(&path_for_closure, &bytes_clone)?;
                                    Ok::<(), std::io::Error>(())
                                }).await;
                                match res {
                                    Ok(Ok(())) => println!("[Node {}] ✓ Saved image to {:?}", me_clone, path_for_log),
                                    Ok(Err(e)) => eprintln!("[Node {}] ✗ Failed to save image: {}", me_clone, e),
                                    Err(e) => eprintln!("[Node {}] ✗ spawn_blocking join error: {}", me_clone, e),
                                }
                            });

                            // Send acknowledgment reply back to client
                            let reply = Message::EncryptReply {
                                req_id: req_id.clone(),
                                ok: true,
                                payload: None,
                                error: None,
                            };

                            let net_reply = net_proc.clone();
                            let req_id_clone = req_id.clone();
                            tokio::spawn(async move {
                                match net_reply.send(from, &reply).await {
                                    Ok(()) => println!("[Node {}] ✓ Sent EncryptReply for {} back to client", me, req_id_clone),
                                    Err(e) => eprintln!("[Node {}] ✗ Failed to send reply for {}: {}", me, req_id_clone, e),
                                }
                            });
                            } else {
                                // Leader forwarding to a different node
                                println!("[Node {}] LEADER: Forwarding request {} to Node {} (least loaded)", me, req_id, target);

                                let msg = Message::EncryptRequest {
                                    from: me,  // Leader is forwarding
                                    req_id: req_id.clone(),
                                    user,
                                    image_bytes,
                                };

                                let net_forward = net_proc.clone();
                                let req_id_fwd = req_id.clone();
                                tokio::spawn(async move {
                                    match net_forward.send(target, &msg).await {
                                        Ok(()) => println!("[Node {}] LEADER: Successfully forwarded request {} to Node {}", me, req_id_fwd, target),
                                        Err(e) => eprintln!("[Node {}] LEADER: Failed to forward request {} to Node {}: {}", me, req_id_fwd, target, e),
                                    }
                                });
                            }
                        } else {
                            // No nodes available - should never happen
                            eprintln!("[Node {}] LEADER: No nodes available for request {}", me, req_id);
                        }
                    } else {
                        // FOLLOWER NODE OR LEADER HANDLING FORWARDED REQUEST
                        // Save the image to shared directory
                        println!("[Node {}] WORKER: Processing request {}", me, req_id);

                        let dir = shared_dir_proc.as_ref().clone();
                        let addr_s = addr.to_string().replace(':', "_");
                        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                        let ext = if image_bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
                            "png"
                        } else if image_bytes.len() > 2 && image_bytes[0] == 0xFF && image_bytes[1] == 0xD8 {
                            "jpg"
                        } else {
                            "bin"
                        };
                        let fname = format!("{}_{}_{}.{}", req_id, addr_s, ts, ext);
                        let path = std::path::Path::new(&dir).join(&fname);
                        let path_for_closure = path.clone();
                        let path_for_log = path.clone();
                        let bytes_clone = image_bytes.clone();
                        let dir_clone = dir.clone();
                        let me_clone = me;

                        tokio::spawn(async move {
                            let res = tokio::task::spawn_blocking(move || {
                                std::fs::create_dir_all(&dir_clone)?;
                                std::fs::write(&path_for_closure, &bytes_clone)?;
                                Ok::<(), std::io::Error>(())
                            }).await;
                            match res {
                                Ok(Ok(())) => println!("[Node {}] ✓ Saved image to {:?}", me_clone, path_for_log),
                                Ok(Err(e)) => eprintln!("[Node {}] ✗ Failed to save image: {}", me_clone, e),
                                Err(e) => eprintln!("[Node {}] ✗ spawn_blocking join error: {}", me_clone, e),
                            }
                        });

                        // Send acknowledgment reply back to the leader (who forwarded this)
                        let reply = Message::EncryptReply {
                            req_id: req_id.clone(),
                            ok: true,
                            payload: None,
                            error: None,
                        };

                        let net_reply = net_proc.clone();
                        let req_id_clone = req_id.clone();
                        tokio::spawn(async move {
                            match net_reply.send(from, &reply).await {
                                Ok(()) => println!("[Node {}] ✓ Sent EncryptReply for {} back to node {}", me, req_id_clone, from),
                                Err(e) => eprintln!("[Node {}] ✗ Failed to send reply for {}: {}", me, req_id_clone, e),
                            }
                        });
                    }
                }

                Message::EncryptReply { req_id, ok, payload, error } => {
                    if ok {
                        println!("[Node {}] ✓ RECEIVED EncryptReply for {} - SUCCESS", me, req_id);
                    } else {
                        println!("[Node {}] ✗ RECEIVED EncryptReply for {} - FAILED: {:?}", me, req_id, error);
                    }
                    if let Some(payload_bytes) = payload {
                        // allow saving replies too if enabled
                        let save_enabled = std::env::var("SAVE_RECEIVED_IMAGES").unwrap_or_default();
                        if save_enabled == "1" || save_enabled.eq_ignore_ascii_case("true") {
                            let dir = std::env::var("SAVE_DIR").unwrap_or_else(|_| "received_images".into());
                            let addr_s = addr.to_string().replace(':', "_");
                            let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                            let ext = if payload_bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
                                "png"
                            } else if payload_bytes.len() > 2 && payload_bytes[0] == 0xFF && payload_bytes[1] == 0xD8 {
                                "jpg"
                            } else {
                                "bin"
                            };
                            let fname = format!("reply_{}_{}_{}.{}", req_id, addr_s, ts, ext);
                            let path = std::path::Path::new(&dir).join(&fname);
                            let path_for_closure = path.clone();
                            let path_for_log = path.clone();
                            let bytes_clone = payload_bytes.clone();
                            let dir_clone = dir.clone();
                            let me_clone = me;
                            tokio::spawn(async move {
                                let res = tokio::task::spawn_blocking(move || {
                                    std::fs::create_dir_all(&dir_clone)?;
                                    std::fs::write(&path_for_closure, &bytes_clone)?;
                                    Ok::<(), std::io::Error>(())
                                }).await;
                                match res {
                                    Ok(Ok(())) => println!("[Node {}] saved reply payload to {:?}", me_clone, path_for_log),
                                    Ok(Err(e)) => eprintln!("[Node {}] failed to save reply payload: {}", me_clone, e),
                                    Err(e) => eprintln!("[Node {}] spawn_blocking join error: {}", me_clone, e),
                                }
                            });
                        }
                    }
                }

                Message::QueryLeader => {
                    println!("[Node {}] QueryLeader request from {}", me, addr);
                    let leader_opt = { leader_proc.read().await.clone() };
                    let reply = Message::LeaderInfo { leader_id: leader_opt, term: 1 };
                    // Spawn a task to send reply (we need to establish connection back to client)
                    // Since client initiated connection, we'll respond on same connection via the channel
                    // But our current architecture doesn't support bidirectional messaging on same connection easily.
                    // Solution: Client should use a separate listener, OR we send via a new connection.
                    // For simplicity, we'll note this limitation and client will need to track responses.
                    // Actually, since the message processor doesn't have access to the framed connection,
                    // we can't reply directly. The client will need to connect to a known port or
                    // we need a different approach. Let's document this and handle in client design.
                    // For now, let's just log - we'll handle this properly when designing client.
                    println!("[Node {}] Current leader: {:?}", me, leader_opt);
                }

                Message::LeaderInfo { leader_id, term: _ } => {
                    // This would be received by a client, not by a server node
                    println!("[Node {}] Received LeaderInfo: leader={:?}", me, leader_id);
                }


            }
        }
    });

    // initial jitter then start election (reduced jitter for faster startup)
    let start_delay = Duration::from_millis(200 + (me as u64 * 50));
    sleep(start_delay).await;

    // Run initial election
    start_election(me, net.clone(), peers.clone(), participating.clone(), ok_received.clone(), leader.clone(), heartbeat_handle.clone()).await;

    // Optional: one-shot image send if environment vars set
    // SEND_IMAGE_PATH=/path/to/img.png SEND_IMAGE_TO=2
    if let Ok(img_path) = std::env::var("SEND_IMAGE_PATH") {
        // Determine target: prefer explicit env var, fall back to least-load balancer
        let target_opt = match std::env::var("SEND_IMAGE_TO") {
            Ok(to_s) => match to_s.parse::<NodeId>() {
                Ok(id) => Some(id),
                Err(_) => {
                    eprintln!("SEND_IMAGE_TO is not a valid NodeId: {}", to_s);
                    None
                }
            },
            Err(_) => {
                // pick automatically using least-load balancer
                balancer.pick(&*peers, me)
            }
        };

        if let Some(to_id) = target_opt {
            // Record the request in the load balancer
            balancer.record_request(to_id);

            let net_clone = net.clone();
            // spawn a task so sending doesn't block the main loop
            tokio::spawn(async move {
                // Use spawn_blocking for file IO because tokio fs feature isn't enabled.
                let path_clone = img_path.clone();
                match tokio::task::spawn_blocking(move || std::fs::read(path_clone)).await {
                    Ok(Ok(bytes)) => {
                        // generate a short random request id
                        let mut rid = [0u8; 16];
                        OsRng.fill_bytes(&mut rid);
                        let req_id = hex::encode(rid);
                        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
                        let msg = Message::EncryptRequest { from: me, req_id: req_id.clone(), user, image_bytes: bytes };
                        match net_clone.send(to_id, &msg).await {
                            Ok(()) => println!("[Node {}] ✓ Sent image request {} -> node {}", me, req_id, to_id),
                            Err(e) => eprintln!("[Node {}] failed to send image to {}: {}", me, to_id, e),
                        }
                    }
                    Ok(Err(e)) => eprintln!("[Node {}] failed to read image {}: {}", me, img_path, e),
                    Err(e) => eprintln!("[Node {}] spawn_blocking failed for {}: {}", me, img_path, e),
                }
            });
        } else {
            eprintln!("No target available for SEND_IMAGE_PATH; no peers to pick from or invalid SEND_IMAGE_TO");
        }
    }

    // REPL: allow sending images while node is running via stdin commands
    // Command: send-image <node_id> <path>
    let net_repl = net.clone();
    let me_repl = me;
    let balancer_repl = balancer.clone();
    let peers_repl = peers.clone();
    // channel to receive lines read by the blocking stdin reader thread
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // spawn a blocking std thread to read stdin lines and forward into the tokio channel
    let tx_clone = cmd_tx.clone();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line_res in stdin.lock().lines() {
            match line_res {
                Ok(line) => {
                    if tx_clone.send(line).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("stdin read error: {}", e);
                    break;
                }
            }
        }
    });

    tokio::spawn(async move {
        println!("[Node {}] REPL ready — commands: send-image <node_id> <path> | help", me_repl);
        while let Some(line) = cmd_rx.recv().await {
            let line = line.trim();
            if line.is_empty() { continue; }
            let mut parts = line.split_whitespace();
            match parts.next() {
                Some("send-image") => {
                    // support: send-image <node_id> <path>
                    //          send-image auto <path>
                    //          send-image <path>  (auto pick)
                    let first = parts.next();
                    let second = parts.next();
                    let (to_token, path) = match (first, second) {
                        (Some(a), Some(b)) => (a.to_string(), b.to_string()),
                        (Some(a), None) => ("auto".to_string(), a.to_string()),
                        _ => {
                            eprintln!("Usage: send-image <node_id> <path> | send-image auto <path> | send-image <path>");
                            continue;
                        }
                    };

                    let to_opt: Option<NodeId> = if to_token.eq_ignore_ascii_case("auto") || to_token == "0" {
                        balancer_repl.pick(&*peers_repl, me_repl)
                    } else {
                        match to_token.parse::<NodeId>() {
                            Ok(id) => Some(id),
                            Err(_) => {
                                eprintln!("Invalid node id: {} (use 'auto' to pick)", to_token);
                                None
                            }
                        }
                    };

                    if let Some(to_id) = to_opt {
                        // Record the request in the load balancer
                        balancer_repl.record_request(to_id);

                        let path = path.to_string();
                        let net_clone = net_repl.clone();
                        tokio::spawn(async move {
                            match tokio::task::spawn_blocking(move || std::fs::read(path)).await {
                                Ok(Ok(bytes)) => {
                                    let byte_count = bytes.len();
                                    let mut rid = [0u8; 16];
                                    OsRng.fill_bytes(&mut rid);
                                    let req_id = hex::encode(rid);
                                    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
                                    let msg = Message::EncryptRequest { from: me_repl, req_id: req_id.clone(), user, image_bytes: bytes };
                                    match net_clone.send(to_id, &msg).await {
                                        Ok(()) => println!("[Node {}] ✓ Sent image request {} -> node {} ({} bytes)", me_repl, req_id, to_id, byte_count),
                                        Err(e) => eprintln!("[Node {}] failed to send image to {}: {}", me_repl, to_id, e),
                                    }
                                }
                                Ok(Err(e)) => eprintln!("[Node {}] failed to read image: {}", me_repl, e),
                                Err(e) => eprintln!("[Node {}] spawn_blocking failed: {}", me_repl, e),
                            }
                        });
                    } else {
                        eprintln!("No eligible target available to send image");
                    }
                }
                Some("help") => {
                    println!("commands:\n  send-image <node_id> <path>\n  help");
                }
                Some(cmd) => {
                    eprintln!("Unknown command: {} (type 'help')", cmd);
                }
                None => {}
            }
        }
        println!("[Node {}] REPL stdin closed", me_repl);
    });

    // Monitor heartbeats: if leader absent for some interval, trigger election
    loop {
        sleep(Duration::from_millis(500)).await;  // Check every 500ms (faster detection)
        let leader_opt = { leader.read().await.clone() };
        let last = { *last_heartbeat.read().await };
        if let Some(ldr) = leader_opt {
            if ldr == me {
                // I'm leader; ensure heartbeat task running
            } else {
                // follower: if no heartbeat for 3 seconds, start election (reduced from 10s)
                let now = Instant::now();
                let stale = last.map(|t| now.duration_since(t) > Duration::from_secs(3)).unwrap_or(true);
                if stale {
                    println!("[Node {}] leader {:?} stale -> starting election", me, leader_opt);
                    start_election(me, net.clone(), peers.clone(), participating.clone(), ok_received.clone(), leader.clone(), heartbeat_handle.clone()).await;
                }
            }
        } else {
            // no leader known, start election
            println!("[Node {}] no leader -> starting election", me);
            start_election(me, net.clone(), peers.clone(), participating.clone(), ok_received.clone(), leader.clone(), heartbeat_handle.clone()).await;
        }
    }
}

/// Start an election: send Election to higher-id nodes and await responses.
async fn start_election(
    me: NodeId,
    net: Net,
    peers: Arc<HashMap<NodeId, SocketAddr>>,
    participating: Arc<RwLock<bool>>,
    ok_received: Arc<AtomicBool>,
    leader: Arc<RwLock<Option<NodeId>>>,
    heartbeat_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
) {
    // Only one election at a time
    {
        let mut p = participating.write().await;
        if *p {
            return;
        }
        *p = true;
    }

    ok_received.store(false, Ordering::SeqCst);

    // Find higher nodes
    let higher: Vec<NodeId> = peers.keys().filter(|&&id| id > me).cloned().collect();

    if higher.is_empty() {
        // we are highest -> become coordinator
        println!("[Node {}] no higher nodes -> becoming coordinator", me);
        {
            let mut g = leader.write().await;
            *g = Some(me);
        }
        // announce to all
        for (&id, _) in peers.iter() {
            if id == me { continue; }
            net.send(id, &Message::Coordinator { leader: me, term: 1 }).await.ok();
        }

        // start heartbeats
    let h = start_heartbeat_task(me, net.clone(), peers.clone());
        *heartbeat_handle.write().await = Some(h);

        // done participating
        *participating.write().await = false;
        return;
    }

    // send Election to all higher nodes
    for id in higher.iter() {
        net.send(*id, &Message::Election { from: me }).await.ok();
    }

    // wait for any Ok for a timeout (reduced from 5s to 2s)
    let wait_dur = Duration::from_secs(2);
    let start = Instant::now();
    while Instant::now().duration_since(start) < wait_dur {
        if ok_received.load(Ordering::SeqCst) {
            // someone higher responded; they will take over the election
            println!("[Node {}] received Ok -> waiting for Coordinator", me);
            // give some time to receive Coordinator (reduced from 8s to 3s)
            sleep(Duration::from_secs(3)).await;
            *participating.write().await = false;
            return;
        }
        sleep(Duration::from_millis(50)).await;  // Check more frequently (50ms instead of 100ms)
    }

    // no Ok received -> become coordinator
    println!("[Node {}] no Ok received -> becoming coordinator", me);
    {
        let mut g = leader.write().await;
        *g = Some(me);
    }
    for (&id, _) in peers.iter() {
        if id == me { continue; }
        net.send(id, &Message::Coordinator { leader: me, term: 1 }).await.ok();
    }
    let h = start_heartbeat_task(me, net.clone(), peers.clone());
    *heartbeat_handle.write().await = Some(h);
    *participating.write().await = false;
}

fn start_heartbeat_task(me: NodeId, net: Net, peers: Arc<HashMap<NodeId, SocketAddr>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        println!("[Node {}] starting heartbeat task", me);
        loop {
            for (&id, _) in peers.iter() {
                if id == me { continue; }
                net.send(id, &Message::Heartbeat { from: me, term: 1 }).await.ok();
            }
            sleep(Duration::from_millis(500)).await;  // Send heartbeat every 500ms (faster than 1s)
        }
    })
}

