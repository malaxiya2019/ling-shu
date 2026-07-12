//! LSDistributed — 分布式运行时 (v5.0).
//!
//! 为 LingShu 提供多节点部署能力，包含 6 个核心子系统：
//!
//! # 模块架构
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │                  LSDistributed                       │
//! │  ┌─────────┐ ┌─────────┐ ┌──────────┐ ┌─────────┐ │
//! │  │ Cluster  │ │ Scheduler│ │  Queue   │ │  Store  │ │
//! │  │ (集群)   │ │ (调度)  │ │ (队列)   │ │ (存储)  │ │
//! │  └────┬────┘ └────┬────┘ └────┬─────┘ └────┬────┘ │
//! │       │           │           │            │       │
//! │  ┌────▼────┐ ┌───▼────┐ ┌───▼──────┐          │
//! │  │ Leader  │ │ Cache  │ │ Gossip   │          │
//! │  │ (选举)  │ │ (缓存) │ │ (协议)   │          │
//! │  └─────────┘ └────────┘ └──────────┘          │
//! └────────────────────────────────────────────────────┘
//! ```
//!
//! # 核心类型
//!
//! | 类型 | 用途 | 说明 |
//! |------|------|------|
//! | [`Cluster`] | 集群管理 | SWIM 故障检测 + Gossip 传播 |
//! | [`ClusterConfig`] | 集群配置 | 节点 ID / 地址 / 心跳 / Gossip 参数 |
//! | [`ClusterState`] | 集群状态 | 节点列表 / 角色 / 状态查询 |
//! | [`NodeRole`] | 节点角色 | Leader / Follower / Observer |
//! | [`NodeStatus`] | 节点状态 | Alive / Suspect / Dead |
//! | [`DistScheduler`] | 分布式调度器 | 多策略调度 + 负载均衡 + 故障转移 |
//! | [`DistSchedulerConfig`] | 调度器配置 | 策略 / 重试 / 超时 / 健康检查 |
//! | [`DistTask`] | 分布式任务 | 类型 / 负载 / 优先级 / 亲缘性 |
//! | [`DistScheduleResult`] | 调度结果 | 分配节点 / 是否本地 |
//! | [`LeaderElection`] | 领导者选举 | Raft 风格 / 租约机制 |
//! | [`DistributedQueue`] | 分布式队列 | 发布 / 订阅 / Ack |
//! | [`DistributedCache`] | 分布式缓存 | TTL / 失效通知 |
//! | [`DistributedStore`] | 分布式 KV 存储 | 强一致性 / 分区容忍 |

pub mod cache;
pub mod cluster;
pub mod leader;
pub mod queue;
pub mod scheduler;
pub mod store;

/// 集群管理 — SWIM 故障检测 + Gossip 传播
pub use cluster::{Cluster, ClusterConfig, ClusterNode, ClusterState, NodeRole, NodeStatus};

/// 领导者选举 — Raft 风格 / 租约机制
pub use leader::{LeaderElection, LeaderElectionConfig, LeaderState};

/// 分布式队列 — 发布 / 订阅 / Ack
pub use queue::{DistributedQueue, QueueConfig, QueueMessage};

/// 分布式调度器 — 多策略 / 负载均衡 / 故障转移
pub use scheduler::*;

/// 分布式 KV 存储 — 强一致性 / 分区容忍
pub use store::{DistributedStore, StoreConfig, StoreValue};

/// 分布式缓存 — TTL / 失效通知
pub use cache::{CacheConfig, CacheEntry, DistributedCache};
