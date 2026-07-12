//! LSFed — 集群发现.
//!
//! 支持静态配置、DNS SRV 记录两种发现机制。
//!
//! ```text
//! ┌──────────────┐
//! │  Discovery   │
//! │  ┌────────┐  │
//! │  │ Static │  │ — 配置文件直连
//! │  ├────────┤  │
//! │  │  DNS   │  │ — SRV 记录解析
//! │  └────────┘  │
//! └──────────────┘
//! ```

use crate::types::FederationNode;
use async_trait::async_trait;
use lingshu_core::{LsId, LsResult};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 发现后端 trait.
#[async_trait]
pub trait DiscoveryBackend: Send + Sync {
    /// 发现节点列表.
    async fn discover(&self) -> LsResult<Vec<DiscoveredNode>>;

    /// 后端名称.
    fn name(&self) -> &'static str;
}

/// 发现到的原始节点信息.
#[derive(Debug, Clone)]
pub struct DiscoveredNode {
    /// 节点标识.
    pub id: String,
    /// 节点名称.
    pub name: String,
    /// 节点连接地址列表.
    pub addrs: Vec<SocketAddr>,
    /// 额外元数据.
    pub metadata: std::collections::HashMap<String, String>,
}

// ── 静态发现 ───────────────────────────────────────

/// 静态配置发现 — 从配置文件中读取种子节点.
pub struct StaticDiscovery {
    seeds: Vec<SocketAddr>,
}

impl StaticDiscovery {
    /// 创建静态发现实例.
    pub fn new(seeds: Vec<SocketAddr>) -> Self {
        Self { seeds }
    }
}

#[async_trait]
impl DiscoveryBackend for StaticDiscovery {
    async fn discover(&self) -> LsResult<Vec<DiscoveredNode>> {
        let nodes: Vec<DiscoveredNode> = self
            .seeds
            .iter()
            .enumerate()
            .map(|(i, addr)| DiscoveredNode {
                id: format!("seed-{i}"),
                name: format!("seed-{i}"),
                addrs: vec![*addr],
                metadata: [("source".into(), "static".into())].into(),
            })
            .collect();
        debug!(count = nodes.len(), "static discovery completed");
        Ok(nodes)
    }

    fn name(&self) -> &'static str {
        "static"
    }
}

// ── DNS 发现 ───────────────────────────────────────

/// DNS SRV 记录发现.
pub struct DnsDiscovery {
    /// 服务域名.
    domain: String,
    /// 服务名称.
    service: String,
    /// 协议 (tcp/udp).
    protocol: String,
}

impl DnsDiscovery {
    /// 创建 DNS 发现实例.
    pub fn new(domain: &str, service: &str) -> Self {
        Self {
            domain: domain.to_string(),
            service: service.to_string(),
            protocol: "tcp".to_string(),
        }
    }

    /// 构建完整的 SRV 查询名.
    fn srv_name(&self) -> String {
        format!("_{}._{}.{}", self.service, self.protocol, self.domain)
    }
}

#[async_trait]
impl DiscoveryBackend for DnsDiscovery {
    async fn discover(&self) -> LsResult<Vec<DiscoveredNode>> {
        let srv_name = self.srv_name();
        let result = tokio::net::lookup_host(&srv_name).await;
        match result {
            Ok(addrs) => {
                let addrs: Vec<SocketAddr> = addrs.collect();
                let nodes = vec![DiscoveredNode {
                    id: format!("dns-{}", self.domain.replace('.', "-")),
                    name: self.domain.clone(),
                    addrs: addrs.clone(),
                    metadata: [("source".into(), "dns".into())].into(),
                }];
                debug!(
                    domain = %self.domain,
                    count = nodes.len(),
                    "DNS discovery completed"
                );
                Ok(nodes)
            }
            Err(e) => {
                warn!(domain = %self.domain, error = %e, "DNS discovery failed");
                Ok(Vec::new())
            }
        }
    }

    fn name(&self) -> &'static str {
        "dns"
    }
}

// ── 发现管理器 ─────────────────────────────────────

/// 发现管理器 — 组合多个发现后端，定期刷新节点列表.
pub struct DiscoveryManager {
    /// 注册的发现后端.
    backends: Vec<Arc<dyn DiscoveryBackend>>,
    /// 发现的节点缓存.
    nodes: Arc<RwLock<Vec<FederationNode>>>,
    /// 本地集群 ID.
    local_cluster_id: LsId,
}

impl DiscoveryManager {
    /// 创建发现管理器.
    pub fn new(local_cluster_id: LsId) -> Self {
        Self {
            backends: Vec::new(),
            nodes: Arc::new(RwLock::new(Vec::new())),
            local_cluster_id,
        }
    }

    /// 注册发现后端.
    pub fn register(&mut self, backend: Arc<dyn DiscoveryBackend>) {
        info!(backend = backend.name(), "discovery backend registered");
        self.backends.push(backend);
    }

    /// 执行一次全量发现.
    pub async fn discover(&self) -> LsResult<Vec<FederationNode>> {
        let mut all_nodes = Vec::new();
        let local_id = self.local_cluster_id;

        for backend in &self.backends {
            match backend.discover().await {
                Ok(raw_nodes) => {
                    for raw in raw_nodes {
                        if raw.id == local_id.to_string() {
                            continue;
                        }
                        let mut node = FederationNode::new(LsId::new(), &raw.name, raw.addrs);
                        node.metadata.extend(raw.metadata);
                        all_nodes.push(node);
                    }
                }
                Err(e) => {
                    warn!(backend = backend.name(), error = %e, "discovery failed");
                }
            }
        }

        // 去重（按名称）
        all_nodes.sort_by(|a, b| a.name.cmp(&b.name));
        all_nodes.dedup_by(|a, b| a.name == b.name);

        let mut cache = self.nodes.write().await;
        *cache = all_nodes.clone();

        info!(count = all_nodes.len(), "discovery cycle completed");
        Ok(all_nodes)
    }

    /// 获取缓存节点.
    pub async fn cached_nodes(&self) -> Vec<FederationNode> {
        self.nodes.read().await.clone()
    }

    /// 启动定期发现循环.
    pub fn start_periodic(
        self: Arc<Self>,
        interval: std::time::Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if let Err(e) = self.discover().await {
                    warn!("periodic discovery failed: {e}");
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_static_discovery() {
        let seeds = vec!["10.0.0.1:9550".parse().unwrap()];
        let discovery = StaticDiscovery::new(seeds);
        let nodes = discovery.discover().await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].addrs[0].to_string(), "10.0.0.1:9550");
    }

    #[tokio::test]
    async fn test_discovery_manager() {
        let mut mgr = DiscoveryManager::new(LsId::new());
        let seeds = vec!["192.168.1.1:9550".parse().unwrap()];
        mgr.register(Arc::new(StaticDiscovery::new(seeds)));

        let nodes = mgr.discover().await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].addrs[0].port(), 9550);
    }

    #[test]
    fn test_dns_srv_name() {
        let dns = DnsDiscovery::new("example.com", "lingshu-fed");
        assert_eq!(dns.srv_name(), "_lingshu-fed._tcp.example.com");
    }
}
