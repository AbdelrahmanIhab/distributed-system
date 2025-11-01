use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::message::NodeId;

/// Simple round-robin balancer.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

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
}
