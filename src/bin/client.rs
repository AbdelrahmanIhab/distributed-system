// Client binary for distributed system
// Connects to the leader server and sends images

use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::time::{sleep, Duration};
use tokio::sync::RwLock;
use anyhow::{Context, Result};
use std::io::BufRead;

// Import from the main crate
use cloud_p2p::{Message, NodeId, Net, Config};
use rand::RngCore;
use rand::rngs::OsRng;
use hex;

#[tokio::main]
async fn main() -> Result<()> {
    println!("[Client] Starting distributed system client...");

    // Load server configuration
    let config_path = std::env::var("CONFIG_FILE").ok();
    let config = Config::load(config_path.as_deref())?;
    let servers = config.to_peer_map()?;

    println!("[Client] Available servers: {:?}", servers);

    // Use same encryption key as servers (demo only - should be from secure config)
    let key = vec![0u8; 32];

    // Client uses ID 0 (not a real node in the cluster)
    let client_id: NodeId = 0;
    let net = Net::new(client_id, servers.clone(), key);

    // Track the current leader
    let leader: Arc<RwLock<Option<NodeId>>> = Arc::new(RwLock::new(None));

    // Discover leader by trying to connect and observing
    println!("[Client] Discovering leader...");
    let discovered_leader = discover_leader(&net, &servers).await?;
    {
        let mut g = leader.write().await;
        *g = Some(discovered_leader);
    }
    println!("[Client] Leader discovered: Node {}", discovered_leader);

    // REPL: Interactive commands
    let net_repl = net.clone();
    let leader_repl = leader.clone();
    let servers_repl = Arc::new(servers.clone());

    // Channel to receive lines from stdin
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Spawn blocking thread to read stdin
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
                    eprintln!("[Client] stdin read error: {}", e);
                    break;
                }
            }
        }
    });

    // Start listening for replies
    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::unbounded_channel::<(SocketAddr, Message)>();
    let reply_net = net.clone();

    // Bind to a client port for receiving replies
    let client_bind: SocketAddr = "127.0.0.1:7100".parse()?;
    let on_reply = move |addr: SocketAddr, msg: Message| {
        let _ = reply_tx.send((addr, msg));
    };

    tokio::spawn(async move {
        if let Err(e) = reply_net.run_listener(client_bind, on_reply).await {
            eprintln!("[Client] listener error: {}", e);
        }
    });

    // Handle incoming replies
    tokio::spawn(async move {
        while let Some((_addr, msg)) = reply_rx.recv().await {
            match msg {
                Message::EncryptReply { req_id, ok, payload: _, error } => {
                    if ok {
                        println!("[Client] ✓ SUCCESS: Image {} was accepted by the server", req_id);
                    } else {
                        println!("[Client] ✗ FAILED: Image {} was rejected: {:?}", req_id, error);
                    }
                }
                _ => {
                    // Ignore other message types
                }
            }
        }
    });

    println!("[Client] REPL ready — commands:");
    println!("  send-image <path>        Send image to leader");
    println!("  discover                 Rediscover leader");
    println!("  status                   Show current leader");
    println!("  help                     Show this help");
    println!();

    // Process REPL commands
    while let Some(line) = cmd_rx.recv().await {
        let line = line.trim();
        if line.is_empty() { continue; }

        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("send-image") => {
                let path = match parts.next() {
                    Some(p) => p.to_string(),
                    None => {
                        eprintln!("[Client] Usage: send-image <path>");
                        continue;
                    }
                };

                let leader_id = {
                    let g = leader_repl.read().await;
                    match *g {
                        Some(id) => id,
                        None => {
                            eprintln!("[Client] No leader known. Run 'discover' first.");
                            continue;
                        }
                    }
                };

                let net_clone = net_repl.clone();
                tokio::spawn(async move {
                    match tokio::task::spawn_blocking(move || std::fs::read(path)).await {
                        Ok(Ok(bytes)) => {
                            let byte_count = bytes.len();
                            let mut rid = [0u8; 16];
                            OsRng.fill_bytes(&mut rid);
                            let req_id = hex::encode(rid);
                            let user = std::env::var("USER").unwrap_or_else(|_| "client".into());

                            // Client sends with from: 0 (client ID)
                            let msg = Message::EncryptRequest {
                                from: 0,
                                req_id: req_id.clone(),
                                user,
                                image_bytes: bytes
                            };

                            match net_clone.send(leader_id, &msg).await {
                                Ok(()) => println!("[Client] ✓ Sent image {} to leader (node {}) — {} bytes", req_id, leader_id, byte_count),
                                Err(e) => eprintln!("[Client] ✗ Failed to send image to leader: {}", e),
                            }
                        }
                        Ok(Err(e)) => eprintln!("[Client] ✗ Failed to read image file: {}", e),
                        Err(e) => eprintln!("[Client] ✗ spawn_blocking failed: {}", e),
                    }
                });
            }
            Some("discover") => {
                println!("[Client] Discovering leader...");
                match discover_leader(&net_repl, &servers_repl).await {
                    Ok(new_leader) => {
                        let mut g = leader_repl.write().await;
                        *g = Some(new_leader);
                        println!("[Client] Leader discovered: Node {}", new_leader);
                    }
                    Err(e) => eprintln!("[Client] Failed to discover leader: {}", e),
                }
            }
            Some("status") => {
                let g = leader_repl.read().await;
                match *g {
                    Some(id) => println!("[Client] Current leader: Node {}", id),
                    None => println!("[Client] No leader known"),
                }
            }
            Some("help") => {
                println!("Commands:");
                println!("  send-image <path>   Send image to leader");
                println!("  discover            Rediscover leader");
                println!("  status              Show current leader");
                println!("  help                Show this help");
            }
            Some(cmd) => {
                eprintln!("[Client] Unknown command: {} (type 'help')", cmd);
            }
            None => {}
        }
    }

    println!("[Client] REPL closed");
    Ok(())
}

/// Discover the current leader by sending Hello to all servers
/// and finding which one is the leader (simple heuristic: highest ID responding)
async fn discover_leader(net: &Net, servers: &HashMap<NodeId, SocketAddr>) -> Result<NodeId> {
    // Strategy: Send Hello to all servers and pick the highest responding node
    // In a real system, servers would reply with their leader info
    // For simplicity, we'll just pick the highest ID that responds (likely the leader with Bully algorithm)

    let mut responding_nodes = Vec::new();

    for &node_id in servers.keys() {
        let msg = Message::Hello { from: 0 };
        match net.send(node_id, &msg).await {
            Ok(()) => {
                println!("[Client] Node {} responded", node_id);
                responding_nodes.push(node_id);
            }
            Err(e) => {
                eprintln!("[Client] Node {} did not respond: {}", node_id, e);
            }
        }
    }

    // Wait a bit for responses
    sleep(Duration::from_millis(500)).await;

    if responding_nodes.is_empty() {
        anyhow::bail!("No servers responded");
    }

    // With Bully algorithm, the highest ID node is likely the leader
    responding_nodes.sort();
    let leader = *responding_nodes.last().unwrap();

    Ok(leader)
}