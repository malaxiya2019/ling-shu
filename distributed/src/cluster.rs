//! Cluster management — node discovery, membership, gossip protocol

use crate::leader::LeaderState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Node role in the cluster
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeRole {
    Leader,
    Follower,
    Candidate,
}

/// Node health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeStatus {
    Alive,
    Suspect,
    Dead,
}

/// A single cluster node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNode {
    pub id: String,
    pub addr: String,
    pub role: NodeRole,
    pub status: NodeStatus,
    pub last_heartbeat: i64,
    pub version: u64,
}

/// Cluster configuration
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    pub node_id: String,
    pub bind_addr: String,
    pub seed_nodes: Vec<String>,
    pub heartbeat_interval: Duration,
    pub suspicion_mult: u32,
    pub cleanup_interval: Duration,
    pub gossip_interval: Duration,
    pub gossip_fanout: usize,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            node_id: uuid::Uuid::new_v4().to_string(),
            bind_addr: "127.0.0.1:0".to_string(),
            seed_nodes: vec![],
            heartbeat_interval: Duration::from_secs(1),
            suspicion_mult: 3,
            cleanup_interval: Duration::from_secs(10),
            gossip_interval: Duration::from_millis(500),
            gossip_fanout: 3,
        }
    }
}

/// Cluster state
pub struct ClusterState {
    config: ClusterConfig,
    local_node: ClusterNode,
    members: HashMap<String, ClusterNode>,
    leader_state: Option<LeaderState>,
}

impl ClusterState {
    pub fn new(config: ClusterConfig) -> Self {
        let local_node = ClusterNode {
            id: config.node_id.clone(),
            addr: config.bind_addr.clone(),
            role: NodeRole::Follower,
            status: NodeStatus::Alive,
            last_heartbeat: chrono::Utc::now().timestamp(),
            version: 0,
        };
        Self {
            config,
            local_node,
            members: HashMap::new(),
            leader_state: None,
        }
    }

    pub fn local_id(&self) -> &str {
        &self.local_node.id
    }

    pub fn local_node(&self) -> &ClusterNode {
        &self.local_node
    }

    pub fn members(&self) -> &HashMap<String, ClusterNode> {
        &self.members
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    pub fn live_members(&self) -> Vec<&ClusterNode> {
        self.members
            .values()
            .filter(|n| n.status == NodeStatus::Alive)
            .collect()
    }

    pub fn add_member(&mut self, node: ClusterNode) {
        debug!("Adding cluster member: {} at {}", node.id, node.addr);
        self.members.insert(node.id.clone(), node);
    }

    pub fn remove_member(&mut self, node_id: &str) {
        info!("Removing cluster member: {}", node_id);
        self.members.remove(node_id);
    }

    pub fn handle_heartbeat(&mut self, node_id: &str, timestamp: i64) {
        if let Some(node) = self.members.get_mut(node_id) {
            node.last_heartbeat = timestamp;
            node.status = NodeStatus::Alive;
        }
    }

    /// Gossip: exchange membership info with a random subset of peers
    pub fn gossip_peer(&self) -> Option<&ClusterNode> {
        let alive: Vec<&String> = self
            .members
            .iter()
            .filter(|(id, n)| *id != &self.local_node.id && n.status == NodeStatus::Alive)
            .map(|(id, _)| id)
            .collect();
        if alive.is_empty() {
            return None;
        }
        let idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % alive.len() as u128) as usize;
        self.members.get(alive[idx])
    }

    /// Detect failed nodes based on heartbeat timeout
    pub fn detect_failures(&mut self) -> Vec<String> {
        let now = chrono::Utc::now().timestamp();
        let timeout =
            self.config.heartbeat_interval.as_secs() as i64 * self.config.suspicion_mult as i64;
        let mut failed = Vec::new();
        for (id, node) in self.members.iter_mut() {
            if node.status == NodeStatus::Alive && (now - node.last_heartbeat) > timeout {
                warn!(
                    "Node {} suspected dead (last heartbeat: {})",
                    id, node.last_heartbeat
                );
                node.status = NodeStatus::Suspect;
            }
            if node.status == NodeStatus::Suspect && (now - node.last_heartbeat) > timeout * 2 {
                error!("Node {} declared dead", id);
                node.status = NodeStatus::Dead;
                failed.push(id.clone());
            }
        }
        failed
    }

    pub fn config(&self) -> &ClusterConfig {
        &self.config
    }

    pub fn set_leader_state(&mut self, state: Option<LeaderState>) {
        self.leader_state = state;
    }

    pub fn leader_state(&self) -> Option<&LeaderState> {
        self.leader_state.as_ref()
    }
}

/// Cluster orchestrator — runs gossip and failure detection loops
pub struct Cluster {
    state: Arc<RwLock<ClusterState>>,
}

impl Cluster {
    pub fn new(config: ClusterConfig) -> Self {
        let state = Arc::new(RwLock::new(ClusterState::new(config)));
        Self { state }
    }

    pub fn state(&self) -> &Arc<RwLock<ClusterState>> {
        &self.state
    }

    /// Start background gossip and failure detection tasks
    pub async fn start(&self) {
        let state = self.state.clone();
        let gossip_interval = {
            let s = state.read().await;
            s.config().gossip_interval
        };

        // Gossip loop
        let state_g = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(gossip_interval).await;
                let mut s = state_g.write().await;
                s.detect_failures();
                if let Some(peer) = s.gossip_peer() {
                    debug!("Gossiping with peer: {} at {}", peer.id, peer.addr);
                }
            }
        });

        info!("Cluster started on node {}", {
            let s = state.read().await;
            s.local_id().to_string()
        });
    }

    /// Join an existing cluster via seed nodes
    pub async fn join(&self, _seeds: &[String]) -> Result<(), String> {
        let mut state = self.state.write().await;
        for seed in _seeds {
            let peer = ClusterNode {
                id: format!("peer-{}", uuid::Uuid::new_v4()),
                addr: seed.clone(),
                role: NodeRole::Follower,
                status: NodeStatus::Alive,
                last_heartbeat: chrono::Utc::now().timestamp(),
                version: 0,
            };
            state.add_member(peer);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_state_init() {
        let config = ClusterConfig::default();
        let state = ClusterState::new(config);
        assert_eq!(state.member_count(), 0);
        assert!(!state.local_id().is_empty());
    }

    #[test]
    fn test_add_remove_member() {
        let mut state = ClusterState::new(ClusterConfig::default());
        let node = ClusterNode {
            id: "node-1".into(),
            addr: "127.0.0.1:8001".into(),
            role: NodeRole::Follower,
            status: NodeStatus::Alive,
            last_heartbeat: chrono::Utc::now().timestamp(),
            version: 0,
        };
        state.add_member(node);
        assert_eq!(state.member_count(), 1);
        state.remove_member("node-1");
        assert_eq!(state.member_count(), 0);
    }

    #[test]
    fn test_heartbeat() {
        let mut state = ClusterState::new(ClusterConfig::default());
        let node = ClusterNode {
            id: "node-1".into(),
            addr: "127.0.0.1:8001".into(),
            role: NodeRole::Follower,
            status: NodeStatus::Alive,
            last_heartbeat: 0,
            version: 0,
        };
        state.add_member(node);
        let now = chrono::Utc::now().timestamp();
        state.handle_heartbeat("node-1", now);
        let node = state.members().get("node-1").unwrap();
        assert_eq!(node.last_heartbeat, now);
    }

    #[test]
    fn test_live_members() {
        let mut state = ClusterState::new(ClusterConfig::default());
        state.add_member(ClusterNode {
            id: "alive-1".into(),
            addr: "127.0.0.1:8001".into(),
            role: NodeRole::Follower,
            status: NodeStatus::Alive,
            last_heartbeat: 100,
            version: 0,
        });
        state.add_member(ClusterNode {
            id: "dead-1".into(),
            addr: "127.0.0.1:8002".into(),
            role: NodeRole::Follower,
            status: NodeStatus::Dead,
            last_heartbeat: 0,
            version: 0,
        });
        assert_eq!(state.live_members().len(), 1);
    }
}
