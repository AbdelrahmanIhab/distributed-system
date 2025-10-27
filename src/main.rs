mod message;
mod net;

use std::{collections::HashMap, net::SocketAddr};
use tokio::time::{sleep, Duration};
use message::{Message, NodeId};
use net::Net;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1️⃣ Read node ID and bind address from environment
    let me: NodeId = std::env::var("NODE_ID").unwrap_or("1".into()).parse()?;
    let bind: SocketAddr = std::env::var("BIND").unwrap_or("127.0.0.1:7001".into()).parse()?;

    // 2️⃣ Hardcode addresses of all peers (you can change these later)
    let mut peers = HashMap::new();
    peers.insert(1, "127.0.0.1:7001".parse()?);
    peers.insert(2, "127.0.0.1:7002".parse()?);
    peers.insert(3, "127.0.0.1:7003".parse()?);

    // 3️⃣ Create the network manager for this node
    let net = Net::new(me, peers);

    // 4️⃣ Define what happens when this node receives a message
    let on_msg = move |addr: SocketAddr, msg: Message| {
        println!("[Node {}] <- from {} : {:?}", me, addr, msg);
    };

    // 5️⃣ Run listener in the background
    let net_listener = net.clone();
    tokio::spawn(async move {
        if let Err(e) = net_listener.run_listener(bind, on_msg).await {
            eprintln!("[Node {}] listener error: {}", me, e);
        }
    });

    // 6️⃣ Simulate message traffic
    if me == 1 {
        println!("[Node 1] sending periodic heartbeats...");
        loop {
            net.send(2, &Message::Heartbeat { from: me, term: 1 }).await.ok();
            net.send(3, &Message::Heartbeat { from: me, term: 1 }).await.ok();
            sleep(Duration::from_secs(1)).await;
        }
    } else {
        // Followers send a Hello once
        net.send(1, &Message::Hello { from: me }).await.ok();
        loop {
            sleep(Duration::from_secs(60)).await;
        }
    }
}
