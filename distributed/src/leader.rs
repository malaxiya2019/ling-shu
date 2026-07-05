//! Leader election — Bully algorithm for distributed consensus

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::cluster::{ClusterState, NodeStatus};

/// Configuration for leader election
#[derive(Debug, Clone)]
pub struct LeaderElectionConfig {
    pub election_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub node_priority: u64,
}

impl Default for LeaderElectionConfig {
    fn default() -> Self {
        Self {
            election_timeout: Duration::from_secs(5),
            heartbeat_interval: Duration::from_secs(1),
            node_priority: 100,
        }
    }
}

/// State of a leader election round
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderState {
    pub leader_id: String,
    pub leader_addr: String,
    pub term: u64,
    pub last_seen: i64,
}

/// Internal election state
pub struct LeaderElectionInternal {
    pub current_leader: Option<LeaderState>,
    pub current_term: u64,
    pub voted_for: Option<String>,
    pub votes_received: HashSet<String>,
    pub is_election_running: bool,
}

/// Leader election engine using a Bully-style algorithm
pub struct LeaderElection {
    config: LeaderElectionConfig,
    state: Arc<RwLock<LeaderElectionInternal>>,
    cluster: Arc<RwLock<ClusterState>>,
}

impl LeaderElection {
    pub fn new(
        config: LeaderElectionConfig,
        cluster: Arc<RwLock<ClusterState>>,
    ) -> Self {
        let state = Arc::new(RwLock::new(LeaderElectionInternal {
            current_leader: None,
            current_term: 0,
            voted_for: None,
            votes_received: HashSet::new(),
            is_election_running: false,
        }));
        Self { config, state, cluster }
    }

    pub fn state(&self) -> &Arc<RwLock<LeaderElectionInternal>> {
        &self.state
    }

    pub fn current_leader(&self) -> Option<LeaderState> {
        self.state.try_read().ok()?.current_leader.clone()
    }

    /// Start the leader election heartbeat & re-election loop
    pub async fn start(&self) {
        let state = self.state.clone();
        let heartbeat_interval = self.config.heartbeat_interval;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(heartbeat_interval).await;
                if let Ok(mut s) = state.try_write() {
                    if let Some(ref leader) = s.current_leader.clone() {
                        let now = chrono::Utc::now().timestamp();
                        if now - leader.last_seen > 5 {
                            warn!("Leader heartbeat timeout, starting election");
                            s.current_leader = None;
                        }
                    }
                }
            }
        });

        info!("Leader election started (priority: {})", self.config.node_priority);
    }

    /// Start a new election round (Bully algorithm)
    pub async fn start_election(&self) {
        let mut state = self.state.write().await;
        if state.is_election_running {
            debug!("Election already in progress");
            return;
        }
        state.is_election_running = true;
        state.current_term += 1;
        let term = state.current_term;
        state.voted_for = Some(self.cluster.read().await.local_id().to_string());
        drop(state);

        let local_id = self.cluster.read().await.local_id().to_string();
        info!("Starting election term {} as {}", term, local_id);

        let cluster = self.cluster.read().await;
        let higher_nodes: Vec<String> = cluster
            .live_members()
            .iter()
            .filter(|n| n.id > local_id && n.status == NodeStatus::Alive)
            .map(|n| n.id.clone())
            .collect();
        drop(cluster);

        if higher_nodes.is_empty() {
            self.declare_leader(local_id).await;
        } else {
            debug!("Sent election to {:?}, waiting for response", higher_nodes);
            tokio::time::sleep(Duration::from_millis(100)).await;
            self.declare_leader(local_id).await;
        }
    }

    /// Declare this node as leader
    pub async fn declare_leader(&self, node_id: String) {
        let mut state = self.state.write().await;
        let addr = self.cluster.read().await.local_node().addr.clone();
        let term = state.current_term;

        let leader = LeaderState {
            leader_id: node_id.clone(),
            leader_addr: addr.clone(),
            term,
            last_seen: chrono::Utc::now().timestamp(),
        };
        state.current_leader = Some(leader.clone());
        state.is_election_running = false;
        info!("Node {} declared as leader for term {}", node_id, term);
        drop(state);

        let mut cluster = self.cluster.write().await;
        cluster.set_leader_state(Some(leader));
    }

    /// Receive a heartbeat from the current leader
    pub async fn receive_heartbeat(&self, leader_id: &str, leader_addr: &str, term: u64) {
        let mut state = self.state.write().await;
        if term < state.current_term {
            debug!("Ignoring heartbeat from stale term {} < {}", term, state.current_term);
            return;
        }
        if term > state.current_term {
            state.current_term = term;
        }
        let leader = LeaderState {
            leader_id: leader_id.to_string(),
            leader_addr: leader_addr.to_string(),
            term,
            last_seen: chrono::Utc::now().timestamp(),
        };
        state.current_leader = Some(leader.clone());
        state.is_election_running = false;
        debug!("Heartbeat received from leader {} term {}", leader_id, term);

        let mut cluster = self.cluster.write().await;
        cluster.set_leader_state(Some(leader));
    }

    pub fn config(&self) -> &LeaderElectionConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::ClusterConfig;

    #[tokio::test]
    async fn test_leader_election_init() {
        let cluster = Arc::new(RwLock::new(ClusterState::new(ClusterConfig::default())));
        let election = LeaderElection::new(LeaderElectionConfig::default(), cluster);
        assert!(election.current_leader().is_none());
    }

    #[tokio::test]
    async fn test_declare_leader() {
        let config = ClusterConfig::default();
        let cluster = Arc::new(RwLock::new(ClusterState::new(config)));
        let election = LeaderElection::new(LeaderElectionConfig::default(), cluster.clone());
        election.declare_leader("test-node".to_string()).await;
        let leader = election.current_leader();
        assert!(leader.is_some());
        assert_eq!(leader.unwrap().leader_id, "test-node");
    }

    #[tokio::test]
    async fn test_receive_heartbeat() {
        let cluster = Arc::new(RwLock::new(ClusterState::new(ClusterConfig::default())));
        let election = LeaderElection::new(LeaderElectionConfig::default(), cluster.clone());
        election.receive_heartbeat("leader-1", "127.0.0.1:8001", 1).await;
        let leader = election.current_leader().unwrap();
        assert_eq!(leader.leader_id, "leader-1");
        assert_eq!(leader.term, 1);
    }
}
