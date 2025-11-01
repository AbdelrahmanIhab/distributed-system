use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;

use crate::message::NodeId;

/// Simple round-robin balancer (kept for backward compatibility).
///
/// - `new()` creates a balancer.
/// - `pick(peers, me)` returns the next NodeId (excluding `me`) or `None` if no other peers.
pub struct RoundRobin {
    counter: AtomicUsize,
}

impl RoundRobin {
    pub fn new() -> Self {
        RoundRobin { counter: AtomicUsize::new(0) }
    }

    /// Pick the next peer in a round-robin fashion.
    ///
    /// This is thread-safe and uses an atomic counter. The set of candidate peers is
    /// derived from the keys of `peers` (excluding `me`) and sorted to ensure deterministic
    /// ordering across runs. If there are no other peers, returns `None`.
    pub fn pick(&self, peers: &HashMap<NodeId, SocketAddr>, me: NodeId) -> Option<NodeId> {
        println!("[RoundRobin] Starting peer selection for node {}", me);
        println!("[RoundRobin] Total peers in cluster: {}", peers.len());

        let mut candidates: Vec<NodeId> = peers.keys().filter(|&&id| id != me).cloned().collect();

        if candidates.is_empty() {
            println!("[RoundRobin] WARNING: No candidates available (only me={}) -> returning None", me);
            println!("[RoundRobin] Cannot perform load balancing with no other peers");
            return None;
        }

        println!("[RoundRobin] Filtering out current node ({}), remaining candidates before sort: {:?}", me, candidates);
        candidates.sort();
        println!("[RoundRobin] Candidates after sorting: {:?}", candidates);

        let n = candidates.len();
        println!("[RoundRobin] Number of eligible peers: {}", n);

        // fetch_add returns previous value; use it to compute a stable index
        let prev = self.counter.fetch_add(1, Ordering::SeqCst);
        println!("[RoundRobin] Current counter value: {}", prev);

        let idx = prev % n;
        println!("[RoundRobin] Calculated index: {} % {} = {}", prev, n, idx);

        let chosen = candidates[idx];
        println!("[RoundRobin] SELECTED: Node {} at index {}", chosen, idx);
        println!("[RoundRobin] Next selection will use counter value {}", prev + 1);
        println!("[RoundRobin] ----------------------------------------");

        Some(chosen)
    }
}

/// Least-load balancer.
///
/// Tracks the number of requests assigned to each server and always picks
/// the server with the least number of handled requests.
///
/// - `new()` creates a balancer with zero load for all nodes.
/// - `pick(peers, me)` returns the NodeId with the least load (excluding `me`).
/// - `record_request(node_id)` increments the request count for the given node.
/// - `get_loads()` returns a snapshot of current loads for all nodes.
pub struct LeastLoad {
    /// Maps each NodeId to its current request count
    loads: RwLock<HashMap<NodeId, usize>>,
}

impl LeastLoad {
    pub fn new() -> Self {
        LeastLoad {
            loads: RwLock::new(HashMap::new()),
        }
    }

    /// Pick the peer with the least load (excluding `me`).
    ///
    /// If multiple peers have the same minimum load, the one with the smallest NodeId is chosen.
    /// Returns `None` if there are no other peers.
    pub fn pick(&self, peers: &HashMap<NodeId, SocketAddr>, me: NodeId) -> Option<NodeId> {
        println!("[LeastLoad] Starting peer selection for node {}", me);
        println!("[LeastLoad] Total peers in cluster: {}", peers.len());

        // Get current load state
        let loads = self.loads.read().unwrap();

        // Find candidates (all peers except me)
        let mut candidates: Vec<NodeId> = peers.keys()
            .filter(|&&id| id != me)
            .cloned()
            .collect();

        if candidates.is_empty() {
            println!("[LeastLoad] WARNING: No candidates available (only me={}) -> returning None", me);
            return None;
        }

        println!("[LeastLoad] Candidates (excluding me={}): {:?}", me, candidates);

        // Sort candidates by load (ascending), then by NodeId for deterministic tie-breaking
        candidates.sort_by_key(|&node_id| {
            let load = loads.get(&node_id).copied().unwrap_or(0);
            (load, node_id)
        });

        // Display current loads
        println!("[LeastLoad] Current loads:");
        for &candidate in &candidates {
            let load = loads.get(&candidate).copied().unwrap_or(0);
            println!("[LeastLoad]   Node {}: {} requests", candidate, load);
        }

        let chosen = candidates[0];
        let chosen_load = loads.get(&chosen).copied().unwrap_or(0);

        println!("[LeastLoad] SELECTED: Node {} with {} requests (least loaded)", chosen, chosen_load);
        println!("[LeastLoad] ----------------------------------------");

        Some(chosen)
    }

    /// Record that a request has been assigned to the given node.
    ///
    /// This increments the load counter for that node.
    pub fn record_request(&self, node_id: NodeId) {
        let mut loads = self.loads.write().unwrap();
        let count = loads.entry(node_id).or_insert(0);
        *count += 1;

        println!("[LeastLoad] RECORDED: Node {} now has {} total requests", node_id, *count);
    }

    /// Get a snapshot of current loads for debugging/monitoring.
    pub fn get_loads(&self) -> HashMap<NodeId, usize> {
        self.loads.read().unwrap().clone()
    }

    /// Reset the load counter for a specific node (useful for node recovery scenarios).
    #[allow(dead_code)]
    pub fn reset_node_load(&self, node_id: NodeId) {
        let mut loads = self.loads.write().unwrap();
        loads.insert(node_id, 0);
        println!("[LeastLoad] RESET: Node {} load counter reset to 0", node_id);
    }

    /// Reset all load counters to zero.
    #[allow(dead_code)]
    pub fn reset_all_loads(&self) {
        let mut loads = self.loads.write().unwrap();
        loads.clear();
        println!("[LeastLoad] RESET: All load counters cleared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rr_cycles() {
        let mut peers: HashMap<NodeId, SocketAddr> = HashMap::new();
        peers.insert(1, "127.0.0.1:7001".parse().unwrap());
        peers.insert(2, "127.0.0.1:7002".parse().unwrap());
        peers.insert(3, "127.0.0.1:7003".parse().unwrap());

        let rr = RoundRobin::new();
        // me = 1 -> candidates [2,3]
        assert_eq!(rr.pick(&peers, 1), Some(2));
        assert_eq!(rr.pick(&peers, 1), Some(3));
        assert_eq!(rr.pick(&peers, 1), Some(2));
    }

    #[test]
    fn least_load_picks_least_loaded() {
        let mut peers: HashMap<NodeId, SocketAddr> = HashMap::new();
        peers.insert(1, "127.0.0.1:7001".parse().unwrap());
        peers.insert(2, "127.0.0.1:7002".parse().unwrap());
        peers.insert(3, "127.0.0.1:7003".parse().unwrap());

        let lb = LeastLoad::new();

        // Initially all nodes have 0 load, should pick lowest ID (2, since me=1)
        assert_eq!(lb.pick(&peers, 1), Some(2));

        // Record some requests
        lb.record_request(2);
        lb.record_request(2);
        lb.record_request(3);

        // Node 2 has 2 requests, Node 3 has 1 request
        // Should pick Node 3 (least loaded)
        assert_eq!(lb.pick(&peers, 1), Some(3));

        // Add more load to Node 3
        lb.record_request(3);
        lb.record_request(3);

        // Now Node 2 has 2 requests, Node 3 has 3 requests
        // Should pick Node 2 (least loaded)
        assert_eq!(lb.pick(&peers, 1), Some(2));
    }

    #[test]
    fn least_load_tie_breaking() {
        let mut peers: HashMap<NodeId, SocketAddr> = HashMap::new();
        peers.insert(1, "127.0.0.1:7001".parse().unwrap());
        peers.insert(2, "127.0.0.1:7002".parse().unwrap());
        peers.insert(3, "127.0.0.1:7003".parse().unwrap());

        let lb = LeastLoad::new();

        // All nodes have equal load (0), should pick lowest NodeId
        assert_eq!(lb.pick(&peers, 1), Some(2));

        // Give them equal load
        lb.record_request(2);
        lb.record_request(3);

        // Both have 1 request, should pick Node 2 (lower ID)
        assert_eq!(lb.pick(&peers, 1), Some(2));
    }

    #[test]
    fn least_load_get_loads() {
        let mut peers: HashMap<NodeId, SocketAddr> = HashMap::new();
        peers.insert(1, "127.0.0.1:7001".parse().unwrap());
        peers.insert(2, "127.0.0.1:7002".parse().unwrap());
        peers.insert(3, "127.0.0.1:7003".parse().unwrap());

        let lb = LeastLoad::new();

        lb.record_request(2);
        lb.record_request(2);
        lb.record_request(3);

        let loads = lb.get_loads();
        assert_eq!(loads.get(&2), Some(&2));
        assert_eq!(loads.get(&3), Some(&1));
        assert_eq!(loads.get(&1), None); // No requests recorded for node 1
    }

    #[test]
    fn least_load_reset() {
        let mut peers: HashMap<NodeId, SocketAddr> = HashMap::new();
        peers.insert(1, "127.0.0.1:7001".parse().unwrap());
        peers.insert(2, "127.0.0.1:7002".parse().unwrap());

        let lb = LeastLoad::new();

        lb.record_request(2);
        lb.record_request(2);
        assert_eq!(lb.get_loads().get(&2), Some(&2));

        lb.reset_node_load(2);
        assert_eq!(lb.get_loads().get(&2), Some(&0));

        lb.record_request(2);
        lb.reset_all_loads();
        assert!(lb.get_loads().is_empty());
    }
}
