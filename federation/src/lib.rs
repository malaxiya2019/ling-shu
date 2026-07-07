//! LSFed — Lingshu 跨集群联邦通信.
//!
//! 连接多个 Lingshu 集群，实现跨集群 Agent 执行、能力发现和状态复制。
//!
//! ## 架构
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                 Federation                        │
//! │  ┌──────────┐  ┌──────────┐  ┌────────────────┐ │
//! │  │Discovery │  │  Link    │  │  Protocol      │ │
//! │  │(发现)    │  │ (连接)   │  │  (消息协议)    │ │
//! │  └──────────┘  └──────────┘  └────────────────┘ │
//! │  ┌──────────┐  ┌──────────┐  ┌────────────────┐ │
//! │  │Executor  │  │Replicat.│  │  Capability    │ │
//! │  │(远程执行) │  │(复制)   │  │  (能力声明)    │ │
//! │  └──────────┘  └──────────┘  └────────────────┘ │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! 通过可选的 Plugin EventBus 发布联邦事件（FederationNodeJoined/FederationNodeLeft/
//! FederationSyncComplete）。

pub mod discovery;
pub mod executor;
pub mod link;
pub mod protocol;
pub mod replication;
pub mod types;

pub use discovery::{DiscoveryBackend, DiscoveryManager, DnsDiscovery, StaticDiscovery};
pub use executor::{RemoteDiscovery, RemoteExecutor};
pub use link::{LinkEvent, LinkManager};
pub use protocol::FederationMessage;
pub use replication::{MemoryStateBackend, ReplicationStrategy, StateBackend, StateReplicator};
pub use types::*;

use lingshu_core::{LsId, LsResult};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// 联邦主入口 — 聚合所有联邦功能.
pub struct Federation {
    /// 本地集群 ID.
    pub cluster_id: LsId,
    /// 联邦配置.
    pub config: FederationConfig,
    /// 发现管理器.
    pub discovery_mgr: Arc<DiscoveryManager>,
    /// 连接管理器.
    pub link_mgr: Arc<LinkManager>,
    /// 远程执行器.
    pub executor: Arc<RemoteExecutor>,
    /// 远端发现.
    pub remote_discovery: Arc<RemoteDiscovery>,
    /// 状态复制器.
    pub replicator: Arc<StateReplicator>,
    /// 联邦统计.
    stats: Arc<RwLock<FederationStats>>,
    /// 启动时间.
    started_at: chrono::DateTime<chrono::Utc>,
    /// 可选的 Plugin EventBus（RwLock 支持 &self 方法）.
    plugin_event_bus: RwLock<Option<Arc<lingshu_plugin::event::EventBus>>>,
}

impl Federation {
    /// 创建联邦实例.
    pub async fn new(cluster_id: LsId, config: FederationConfig) -> Self {
        let cluster_name = config.cluster_name.clone();

        let discovery_mgr = Arc::new(DiscoveryManager::new(cluster_id));
        let link_mgr = Arc::new(LinkManager::new(cluster_id, &cluster_name, config.clone()));
        let executor = Arc::new(RemoteExecutor::new(link_mgr.clone()));
        let remote_discovery = Arc::new(RemoteDiscovery::new(link_mgr.clone()));

        let state_backend = Arc::new(MemoryStateBackend::new());
        let replicator = Arc::new(StateReplicator::new(link_mgr.clone(), state_backend));

        let stats = Arc::new(RwLock::new(FederationStats {
            connected_nodes: 0,
            total_nodes: 0,
            total_capabilities: 0,
            total_messages: 0,
            total_errors: 0,
            active_links: 0,
            p50_latency_ms: 0.0,
            uptime_seconds: 0,
        }));

        info!(
            cluster = %cluster_name,
            listen = %config.listen_addr,
            topology = %config.topology.as_str(),
            "federation initialized"
        );

        Self {
            cluster_id,
            config,
            discovery_mgr,
            link_mgr,
            executor,
            remote_discovery,
            replicator,
            stats,
            started_at: chrono::Utc::now(),
            plugin_event_bus: RwLock::new(None),
        }
    }

    /// 设置 Plugin EventBus（运行时注入）.
    pub async fn set_event_bus(&self, event_bus: Arc<lingshu_plugin::event::EventBus>) {
        *self.plugin_event_bus.write().await = Some(event_bus);
    }

    /// 发布联邦事件到 Plugin EventBus.
    async fn emit_event(
        &self,
        event_type: lingshu_plugin::event::EventType,
        payload: serde_json::Value,
    ) {
        if let Some(ref bus) = *self.plugin_event_bus.read().await {
            let source = format!("federation:{}", self.cluster_id);
            let event = lingshu_plugin::event::Event::new(event_type, source, payload);
            bus.publish(&event).await;
        }
    }

    /// 启动联邦（监听 + 心跳 + 发现）.
    pub async fn start(&self) -> LsResult<()> {
        if !self.config.enabled {
            info!("federation is disabled");
            return Ok(());
        }

        self.link_mgr.start_server().await?;

        let mgr = self.discovery_mgr.clone();
        let discover_interval = self.config.discovery_interval;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(discover_interval).await;
                if let Err(e) = mgr.discover().await {
                    warn!("discovery cycle failed: {e}");
                }
            }
        });

        let link_mgr = self.link_mgr.clone();
        let cluster_id = self.cluster_id.to_string();
        let heartbeat_interval = self.config.heartbeat_interval;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(heartbeat_interval).await;
                let msg = FederationMessage::Heartbeat(protocol::HeartbeatPayload {
                    cluster_id: cluster_id.clone(),
                    timestamp: chrono::Utc::now().timestamp(),
                    load: protocol::LoadInfo {
                        active_connections: 0,
                        pending_tasks: 0,
                        cpu_percent: 0.0,
                        memory_percent: 0.0,
                    },
                });
                link_mgr.broadcast(msg);
            }
        });

        if let Err(e) = self.discovery_mgr.discover().await {
            warn!("initial discovery failed: {e}");
        }

        self.link_mgr.connect_all().await;

        // 发布联邦启动事件
        self.emit_event(
            lingshu_plugin::event::EventType::FederationNodeJoined,
            serde_json::json!({
                "cluster_id": self.cluster_id.to_string(),
                "cluster_name": self.config.cluster_name,
                "action": "started",
            }),
        ).await;

        info!("federation started");
        Ok(())
    }

    /// 执行一次发现.
    pub async fn discover(&self) -> LsResult<Vec<FederationNode>> {
        let nodes = self.discovery_mgr.discover().await?;
        if !nodes.is_empty() {
            self.emit_event(
                lingshu_plugin::event::EventType::FederationNodeJoined,
                serde_json::json!({
                    "nodes_discovered": nodes.len(),
                    "cluster_id": self.cluster_id.to_string(),
                }),
            ).await;
        }
        Ok(nodes)
    }

    /// 在远端集群上执行.
    pub async fn execute(
        &self,
        target_cluster: &str,
        target: &str,
        payload: serde_json::Value,
        timeout_secs: u64,
    ) -> LsResult<RemoteExecResponse> {
        self.executor
            .execute(target_cluster, target, payload, timeout_secs)
            .await
    }

    /// 获取联邦统计.
    pub async fn stats(&self) -> FederationStats {
        let mut stats = self.stats.write().await;
        stats.connected_nodes = self.link_mgr.online_nodes().await.len();
        stats.total_nodes = self.discovery_mgr.cached_nodes().await.len();
        stats.active_links = self.link_mgr.all_links().await.len();
        stats.uptime_seconds = (chrono::Utc::now() - self.started_at)
            .num_seconds()
            .max(0) as u64;
        stats.clone()
    }

    /// 在线节点列表.
    pub async fn online_nodes(&self) -> Vec<FederationNode> {
        self.link_mgr.online_nodes().await
    }

    /// 停止联邦所有后台任务（监听、心跳、发现）.
    pub async fn stop(&self) {
        info!("federation stopping");

        // 发布联邦停止事件
        self.emit_event(
            lingshu_plugin::event::EventType::FederationNodeLeft,
            serde_json::json!({
                "cluster_id": self.cluster_id.to_string(),
                "cluster_name": self.config.cluster_name,
                "action": "stopped",
            }),
        ).await;

        self.link_mgr.stop().await;
        info!("federation stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_federation_create() {
        let config = FederationConfig::default();
        let fed = Federation::new(LsId::new(), config).await;
        let stats = fed.stats().await;
        assert_eq!(stats.connected_nodes, 0);
        assert_eq!(stats.total_nodes, 0);
    }

    #[tokio::test]
    async fn test_federation_disabled() {
        let mut config = FederationConfig::default();
        config.enabled = false;
        let fed = Federation::new(LsId::new(), config).await;
        let nodes = fed.online_nodes().await;
        assert!(nodes.is_empty());
    }

    #[tokio::test]
    async fn test_with_event_bus() {
        use lingshu_plugin::event::EventBus;
        let bus = Arc::new(lingshu_plugin::event::EventBus::new());
        let config = FederationConfig::default();
        let fed = Federation::new(LsId::new(), config).await;
        fed.set_event_bus(bus).await;
        let stats = fed.stats().await;
        assert_eq!(stats.connected_nodes, 0);
    }
}
