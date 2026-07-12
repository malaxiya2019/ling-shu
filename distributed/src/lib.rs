//! LSDistributed — Distributed runtime for LingShu.
//!
//! Provides cluster management, leader election, distributed
//! queue, cache, key-value store, and distributed agent scheduler
//! for multi-node deployment.

pub mod cache;
pub mod cluster;
pub mod leader;
pub mod queue;
pub mod scheduler;
pub mod store;

pub use cache::{CacheConfig, CacheEntry, DistributedCache};
pub use cluster::{ClusterConfig, ClusterNode, ClusterState, NodeRole, NodeStatus};
pub use leader::{LeaderElection, LeaderElectionConfig, LeaderState};
pub use queue::{DistributedQueue, QueueConfig, QueueMessage};
pub use scheduler::*;
pub use store::{DistributedStore, StoreConfig, StoreValue};
