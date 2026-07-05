//! LSDistributed — Distributed runtime for LingShu.
//!
//! Provides cluster management, leader election, distributed
//! queue, cache, and key-value store for multi-node deployment.

pub mod cluster;
pub mod leader;
pub mod queue;
pub mod cache;
pub mod store;

pub use cluster::{ClusterConfig, ClusterNode, ClusterState, NodeRole, NodeStatus};
pub use leader::{LeaderElection, LeaderElectionConfig, LeaderState};
pub use queue::{DistributedQueue, QueueConfig, QueueMessage};
pub use cache::{DistributedCache, CacheConfig, CacheEntry};
pub use store::{DistributedStore, StoreConfig, StoreValue};
