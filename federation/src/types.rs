//! LSFed — 联邦核心类型.
//!
//! ## 数据模型
//!
//! ```text
//! ┌─────────────┐     ┌────────────────┐     ┌──────────────────┐
//! │ Federation  │ ──→ │ FederationNode │ ──→ │   Capability     │
//! │ (拓扑管理)   │     │ (集群节点)     │     │ (能力声明)       │
//! └─────────────┘     └────────────────┘     └──────────────────┘
//!                              │
//!                              ▼
//!                     ┌────────────────┐
//!                     │ FederationLink │
//!                     │ (跨集群连接)   │
//!                     └────────────────┘
//! ```

use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

// ── 联邦拓扑 ───────────────────────────────────────

/// 联邦拓扑类型.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FederationTopology {
    /// 全连接 Mesh — 每个节点与所有其他节点直连.
    Mesh,
    /// 星型 Hub-Spoke — 中心节点中转所有通信.
    HubSpoke,
    /// 部分连接 — 通过静态配置的邻居.
    Partial,
}

impl FederationTopology {
    /// 返回拓扑类型的静态字符串表示.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::HubSpoke => "hub_spoke",
            Self::Partial => "partial",
        }
    }
}

// ── 联邦节点 ───────────────────────────────────────

/// 联邦节点状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FederationNodeStatus {
    /// 在线.
    Online,
    /// 离线.
    Offline,
    /// 连接中.
    Connecting,
    /// 不健康.
    Unhealthy,
}

/// 联邦节点 — 代表一个远程 Lingshu 集群.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationNode {
    /// 集群唯一标识.
    pub cluster_id: LsId,
    /// 集群名称.
    pub name: String,
    /// 集群版本.
    pub version: String,
    /// 连接地址.
    pub addrs: Vec<SocketAddr>,
    /// 节点状态.
    pub status: FederationNodeStatus,
    /// 能力声明.
    pub capabilities: Vec<Capability>,
    /// 延迟（RTT 估计）.
    pub latency_ms: u64,
    /// 最后心跳时间.
    pub last_seen: chrono::DateTime<chrono::Utc>,
    /// 元数据.
    pub metadata: HashMap<String, String>,
    /// 重连次数.
    pub reconnect_count: u32,
}

impl FederationNode {
    /// 创建新联邦节点.
    pub fn new(cluster_id: LsId, name: &str, addrs: Vec<SocketAddr>) -> Self {
        Self {
            cluster_id,
            name: name.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            addrs,
            status: FederationNodeStatus::Connecting,
            capabilities: Vec::new(),
            latency_ms: 0,
            last_seen: chrono::Utc::now(),
            metadata: HashMap::new(),
            reconnect_count: 0,
        }
    }

    /// 是否健康可服务.
    pub fn is_healthy(&self) -> bool {
        self.status == FederationNodeStatus::Online
    }

    /// 是否拥有指定能力.
    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.name == cap || c.id == cap)
    }
}

// ── 能力声明 ───────────────────────────────────────

/// 能力类型.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityType {
    /// LLM 模型.
    LlmModel,
    /// Embedding 模型.
    EmbeddingModel,
    /// Agent 类型.
    Agent,
    /// 工具.
    Tool,
    /// MCP 资源.
    Resource,
    /// 存储后端.
    Storage,
    /// 自定义.
    Custom,
}

/// 能力声明 — 集群对外提供的功能.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// 能力标识.
    pub id: String,
    /// 能力名称.
    pub name: String,
    /// 能力类型.
    pub cap_type: CapabilityType,
    /// 版本.
    pub version: String,
    /// 描述.
    pub description: String,
    /// 每秒最大请求数.
    pub max_rps: u64,
    /// 是否已认证（需要凭证）.
    pub authenticated: bool,
    /// 额外配置.
    pub config: HashMap<String, String>,
}

impl Capability {
    /// 创建新的能力声明.
    pub fn new(id: &str, name: &str, cap_type: CapabilityType) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            cap_type,
            version: "1.0.0".to_string(),
            description: String::new(),
            max_rps: 100,
            authenticated: true,
            config: HashMap::new(),
        }
    }
}

// ── 联邦链路 ───────────────────────────────────────

/// 链路健康状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkHealth {
    Healthy,
    Degraded,
    Down,
}

/// 联邦链路 — 两个集群间的连接.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationLink {
    /// 本地集群 ID.
    pub local_cluster: LsId,
    /// 远端集群 ID.
    pub remote_cluster: LsId,
    /// 链路健康状态.
    pub health: LinkHealth,
    /// 建立时间.
    pub established_at: chrono::DateTime<chrono::Utc>,
    /// 消息计数（本端 → 远端）.
    pub messages_sent: u64,
    /// 消息计数（远端 → 本端）.
    pub messages_received: u64,
    /// 总字节发送.
    pub bytes_sent: u64,
    /// 总字节接收.
    pub bytes_received: u64,
    /// 错误计数.
    pub error_count: u64,
    /// 平均延迟.
    pub avg_latency_ms: f64,
}

// ── 联邦配置 ───────────────────────────────────────

/// 联邦配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    /// 本地集群名称.
    pub cluster_name: String,
    /// 监听地址（联邦通信端口）.
    pub listen_addr: SocketAddr,
    /// 拓扑类型.
    pub topology: FederationTopology,
    /// 种子节点列表（用于初始发现）.
    pub seed_nodes: Vec<SocketAddr>,
    /// 自动发现间隔.
    pub discovery_interval: Duration,
    /// 心跳间隔.
    pub heartbeat_interval: Duration,
    /// 心跳超时（判定节点离线）.
    pub heartbeat_timeout: Duration,
    /// 能力广播间隔.
    pub capability_advertise_interval: Duration,
    /// 最大重连尝试.
    pub max_reconnect_attempts: u32,
    /// 重连退避（秒）.
    pub reconnect_backoff_secs: u64,
    /// 是否启用联邦.
    pub enabled: bool,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            cluster_name: String::new(),
            listen_addr: "0.0.0.0:9550".parse().unwrap(),
            topology: FederationTopology::Mesh,
            seed_nodes: Vec::new(),
            discovery_interval: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(15),
            capability_advertise_interval: Duration::from_secs(60),
            max_reconnect_attempts: 5,
            reconnect_backoff_secs: 10,
            enabled: true,
        }
    }
}

// ── 联邦统计 ───────────────────────────────────────

/// 联邦统计汇总.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationStats {
    /// 已连接节点数.
    pub connected_nodes: usize,
    /// 总节点数.
    pub total_nodes: usize,
    /// 总能力数.
    pub total_capabilities: usize,
    /// 总消息数.
    pub total_messages: u64,
    /// 总错误数.
    pub total_errors: u64,
    /// 活跃链路数.
    pub active_links: usize,
    /// 当前联邦延迟 P50.
    pub p50_latency_ms: f64,
    /// 启动时间.
    pub uptime_seconds: u64,
}

// ── 远程执行请求/响应 ─────────────────────────────

/// 远程执行请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteExecRequest {
    /// 请求 ID.
    pub request_id: String,
    /// 目标 Agent/工具名称.
    pub target: String,
    /// 输入参数.
    pub payload: serde_json::Value,
    /// 超时（秒）.
    pub timeout_secs: u64,
    /// 是否流式返回.
    pub stream: bool,
}

/// 远程执行响应.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteExecResponse {
    /// 请求 ID.
    pub request_id: String,
    /// 执行结果.
    pub result: serde_json::Value,
    /// 是否成功.
    pub success: bool,
    /// 错误信息.
    pub error: Option<String>,
    /// 执行延迟（毫秒）.
    pub latency_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_node() {
        let node = FederationNode::new(
            LsId::new(),
            "cluster-east",
            vec!["10.0.0.1:9550".parse().unwrap()],
        );
        assert_eq!(node.name, "cluster-east");
        assert_eq!(node.status, FederationNodeStatus::Connecting);
        assert_eq!(node.addrs.len(), 1);
    }

    #[test]
    fn test_capability() {
        let cap = Capability::new("gpt-4o", "GPT-4o", CapabilityType::LlmModel);
        assert_eq!(cap.name, "GPT-4o");
        assert_eq!(cap.max_rps, 100);
    }

    #[test]
    fn test_federation_config_default() {
        let config = FederationConfig::default();
        assert_eq!(config.topology, FederationTopology::Mesh);
        assert!(config.enabled);
        assert_eq!(config.listen_addr.port(), 9550);
    }
}
