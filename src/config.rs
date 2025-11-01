use std::collections::HashMap;
use std::net::SocketAddr;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::message::NodeId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PeerConfig {
    pub id: NodeId,
    pub address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub peers: Vec<PeerConfig>,
}

impl Config {
    /// Load config from a JSON file
    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;
        let config: Config = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path))?;
        Ok(config)
    }

    /// Load config from environment variable PEERS
    /// Format: PEERS="1=192.168.1.10:7001,2=192.168.1.11:7002,3=192.168.1.12:7003"
    pub fn from_env() -> Result<Self> {
        let peers_str = std::env::var("PEERS")
            .context("PEERS environment variable not set")?;

        let mut peers = Vec::new();
        for peer_str in peers_str.split(',') {
            let parts: Vec<&str> = peer_str.split('=').collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid peer format: {}. Expected format: id=address", peer_str);
            }
            let id: NodeId = parts[0].trim().parse()
                .with_context(|| format!("Invalid node ID: {}", parts[0]))?;
            let address = parts[1].trim().to_string();

            // Validate address format
            let _: SocketAddr = address.parse()
                .with_context(|| format!("Invalid socket address: {}", address))?;

            peers.push(PeerConfig { id, address });
        }

        Ok(Config { peers })
    }

    /// Convert to HashMap for easy lookup
    pub fn to_peer_map(&self) -> Result<HashMap<NodeId, SocketAddr>> {
        let mut map = HashMap::new();
        for peer in &self.peers {
            let addr: SocketAddr = peer.address.parse()
                .with_context(|| format!("Invalid address for node {}: {}", peer.id, peer.address))?;
            map.insert(peer.id, addr);
        }
        Ok(map)
    }

    /// Load config with fallback priority: file -> env -> default localhost
    pub fn load(file_path: Option<&str>) -> Result<Self> {
        // Try file first if provided
        if let Some(path) = file_path {
            if std::path::Path::new(path).exists() {
                println!("[Config] Loading from file: {}", path);
                return Self::from_file(path);
            } else {
                println!("[Config] File {} not found, trying environment", path);
            }
        }

        // Try environment variable
        if std::env::var("PEERS").is_ok() {
            println!("[Config] Loading from PEERS environment variable");
            return Self::from_env();
        }

        // Fall back to default localhost config (for local testing)
        println!("[Config] No config file or env found, using default localhost");
        Ok(Config {
            peers: vec![
                PeerConfig { id: 1, address: "127.0.0.1:7001".to_string() },
                PeerConfig { id: 2, address: "127.0.0.1:7002".to_string() },
                PeerConfig { id: 3, address: "127.0.0.1:7003".to_string() },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_parsing() {
        std::env::set_var("PEERS", "1=192.168.1.10:7001,2=192.168.1.11:7002,3=192.168.1.12:7003");
        let config = Config::from_env().unwrap();
        assert_eq!(config.peers.len(), 3);
        assert_eq!(config.peers[0].id, 1);
        assert_eq!(config.peers[0].address, "192.168.1.10:7001");
    }

    #[test]
    fn test_to_peer_map() {
        let config = Config {
            peers: vec![
                PeerConfig { id: 1, address: "192.168.1.10:7001".to_string() },
                PeerConfig { id: 2, address: "192.168.1.11:7002".to_string() },
            ],
        };
        let map = config.to_peer_map().unwrap();
        assert_eq!(map.len(), 2);
        assert!(map.contains_key(&1));
        assert!(map.contains_key(&2));
    }
}
