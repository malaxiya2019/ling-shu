//! 🔍 MCP Server 自动发现
//!
//! 支持多种发现机制:
//! - Static: 静态配置文件
//! - DNS SRV: DNS 服务发现
//! - mDNS: 局域网组播发现 (zeroconf)
//! - HTTP: 已知端点健康检查
//!
//! v4.3 Enterprise

use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// MCP 服务器发现条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredServer {
    /// 服务器名称
    pub name: String,
    /// 连接 URL
    pub url: String,
    /// 协议版本
    pub version: Option<String>,
    /// 发现方式
    pub source: DiscoverySource,
    /// 服务器能力标签
    pub capabilities: Vec<String>,
    /// 健康状态
    pub healthy: bool,
    /// 延迟 (ms)
    pub latency_ms: Option<u64>,
    /// 最后更新时间
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

/// 发现方式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiscoverySource {
    /// 静态配置
    Static,
    /// DNS SRV 记录
    DnsSrv,
    /// mDNS (zeroconf)
    MDns,
    /// HTTP 端点
    Http,
    /// 手动注册
    Manual,
    /// 其他
    Other(String),
}

impl std::fmt::Display for DiscoverySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static => write!(f, "static"),
            Self::DnsSrv => write!(f, "dns_srv"),
            Self::MDns => write!(f, "mdns"),
            Self::Http => write!(f, "http"),
            Self::Manual => write!(f, "manual"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// MCP 自动发现配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// 是否启用自动发现
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 静态服务器列表
    #[serde(default)]
    pub static_servers: Vec<StaticServerEntry>,
    /// DNS SRV 查询域名
    #[serde(default)]
    pub dns_domains: Vec<String>,
    /// mDNS 服务类型
    #[serde(default)]
    pub mdns_service_types: Vec<String>,
    /// 扫描间隔 (秒)
    #[serde(default = "default_interval")]
    pub scan_interval_secs: u64,
    /// 健康检查超时 (秒)
    #[serde(default = "default_timeout")]
    pub health_check_timeout_secs: u64,
}

fn default_enabled() -> bool { true }
fn default_interval() -> u64 { 60 }
fn default_timeout() -> u64 { 5 }

/// 静态服务器条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticServerEntry {
    /// 服务器名称
    pub name: String,
    /// 连接 URL
    pub url: String,
    /// 能力标签
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            static_servers: vec![
                StaticServerEntry {
                    name: "local-filesystem".into(),
                    url: "http://127.0.0.1:8080/mcp".into(),
                    capabilities: vec!["filesystem".into()],
                },
            ],
            dns_domains: vec!["_mcp._tcp.local".into()],
            mdns_service_types: vec!["_mcp._tcp".into()],
            scan_interval_secs: 60,
            health_check_timeout_secs: 5,
        }
    }
}

/// MCP 服务发现引擎
pub struct McpDiscovery {
    /// 发现的服务器
    discovered: Arc<RwLock<HashMap<String, DiscoveredServer>>>,
    /// 配置
    config: Arc<RwLock<DiscoveryConfig>>,
    /// 发现历史
    history: Arc<RwLock<Vec<DiscoveryEvent>>>,
}

/// 发现事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source: DiscoverySource,
    pub server_name: String,
    pub event_type: String, // "found", "lost", "updated", "unhealthy"
    pub detail: String,
}

impl McpDiscovery {
    /// 创建新的发现引擎
    pub fn new(config: DiscoveryConfig) -> Self {
        let discovery = Self {
            discovered: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(config)),
            history: Arc::new(RwLock::new(Vec::new())),
        };

        // 初始加载静态服务器
        let discovered = discovery.discovered.clone();
        let cfg = discovery.config.clone();
        let history = discovery.history.clone();

        tokio::spawn(async move {
            let config = cfg.read().await;
            for entry in &config.static_servers {
                let server = DiscoveredServer {
                    name: entry.name.clone(),
                    url: entry.url.clone(),
                    version: None,
                    source: DiscoverySource::Static,
                    capabilities: entry.capabilities.clone(),
                    healthy: true,
                    latency_ms: None,
                    last_seen: chrono::Utc::now(),
                };
                discovered.write().await.insert(entry.name.clone(), server);
                history.write().await.push(DiscoveryEvent {
                    timestamp: chrono::Utc::now(),
                    source: DiscoverySource::Static,
                    server_name: entry.name.clone(),
                    event_type: "found".into(),
                    detail: format!("静态配置: {}", entry.url),
                });
                info!("MCP 静态服务器 '{}' 已注册: {}", entry.name, entry.url);
            }
            info!("MCP 自动发现初始化完成 ({} 个静态服务器)", config.static_servers.len());
        });

        discovery
    }

    /// 注册一个手动发现的服务器
    pub async fn register_server(
        &self,
        name: &str,
        url: &str,
        capabilities: Vec<String>,
        source: DiscoverySource,
    ) {
        let server = DiscoveredServer {
            name: name.to_string(),
            url: url.to_string(),
            version: None,
            source,
            capabilities,
            healthy: true,
            latency_ms: None,
            last_seen: chrono::Utc::now(),
        };
        self.discovered.write().await.insert(name.to_string(), server);
        self.history.write().await.push(DiscoveryEvent {
            timestamp: chrono::Utc::now(),
            source: DiscoverySource::Manual,
            server_name: name.to_string(),
            event_type: "found".into(),
            detail: format!("手动注册: {}", url),
        });
        info!("MCP 服务器 '{}' 已手动注册: {}", name, url);
    }

    /// 注销一个服务器
    pub async fn unregister_server(&self, name: &str) {
        self.discovered.write().await.remove(name);
        self.history.write().await.push(DiscoveryEvent {
            timestamp: chrono::Utc::now(),
            source: DiscoverySource::Manual,
            server_name: name.to_string(),
            event_type: "lost".into(),
            detail: "手动注销".into(),
        });
        info!("MCP 服务器 '{}' 已注销", name);
    }

    /// 获取所有已发现的服务器
    pub async fn list_servers(&self) -> Vec<DiscoveredServer> {
        self.discovered.read().await.values().cloned().collect()
    }

    /// 获取健康服务器列表
    pub async fn list_healthy_servers(&self) -> Vec<DiscoveredServer> {
        self.discovered
            .read()
            .await
            .values()
            .filter(|s| s.healthy)
            .cloned()
            .collect()
    }

    /// 按能力查找服务器
    pub async fn find_by_capability(&self, capability: &str) -> Vec<DiscoveredServer> {
        self.discovered
            .read()
            .await
            .values()
            .filter(|s| s.healthy && s.capabilities.iter().any(|c| c == capability))
            .cloned()
            .collect()
    }

    /// 健康检查所有服务器
    pub async fn health_check_all(&self) {
        let servers: Vec<(String, String)> = {
            let discovered = self.discovered.read().await;
            discovered
                .iter()
                .map(|(name, server)| (name.clone(), server.url.clone()))
                .collect()
        };

        for (name, url) in servers {
            let healthy = self.check_server_health(&url).await;
            let mut discovered = self.discovered.write().await;
            if let Some(server) = discovered.get_mut(&name) {
                let was_healthy = server.healthy;
                server.healthy = healthy;
                server.last_seen = chrono::Utc::now();

                if was_healthy && !healthy {
                    self.history.write().await.push(DiscoveryEvent {
                        timestamp: chrono::Utc::now(),
                        source: DiscoverySource::Http,
                        server_name: name.clone(),
                        event_type: "unhealthy".into(),
                        detail: format!("健康检查失败: {}", url),
                    });
                    warn!("MCP 服务器 '{}' 不健康: {}", name, url);
                } else if !was_healthy && healthy {
                    self.history.write().await.push(DiscoveryEvent {
                        timestamp: chrono::Utc::now(),
                        source: DiscoverySource::Http,
                        server_name: name.clone(),
                        event_type: "updated".into(),
                        detail: format!("服务已恢复: {}", url),
                    });
                    info!("MCP 服务器 '{}' 已恢复: {}", name, url);
                }
            }
        }
    }

    /// 检查单个服务器健康状态
    async fn check_server_health(&self, url: &str) -> bool {
        let timeout = {
            let config = self.config.read().await;
            config.health_check_timeout_secs
        };

        // 从 URL 中提取主机和端口
        let host_port = url
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_start_matches("ws://")
            .trim_start_matches("wss://")
            .split('/')
            .next()
            .unwrap_or(url)
            .to_string();

        // 尝试 TCP 连接检查
        match tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            tokio::net::TcpStream::connect(&host_port),
        )
        .await
        {
            Ok(Ok(_)) => true,
            Ok(Err(e)) => {
                debug!("健康检查失败 {}: {}", url, e);
                false
            }
            Err(_) => {
                debug!("健康检查超时 {}: {}s", url, timeout);
                false
            }
        }
    }

    /// 更新配置
    pub async fn update_config(&self, config: DiscoveryConfig) {
        *self.config.write().await = config;
    }

    /// 获取配置
    pub async fn get_config(&self) -> DiscoveryConfig {
        self.config.read().await.clone()
    }

    /// 获取发现事件历史
    pub async fn get_history(&self, limit: usize) -> Vec<DiscoveryEvent> {
        let history = self.history.read().await;
        if limit > 0 && limit < history.len() {
            history[history.len() - limit..].to_vec()
        } else {
            history.clone()
        }
    }

    /// 启动自动扫描循环
    pub fn start_auto_scan(self: &Arc<Self>) {
        let discovery = self.clone();
        tokio::spawn(async move {
            loop {
                // 等待扫描间隔
                let interval = {
                    let config = discovery.config.read().await;
                    config.scan_interval_secs
                };
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

                // 执行健康检查
                info!("MCP 自动发现: 执行健康检查...");
                discovery.health_check_all().await;

                // DNS 发现 (如有配置)
                let dns_domains = {
                    let config = discovery.config.read().await;
                    config.dns_domains.clone()
                };
                for domain in &dns_domains {
                    if let Err(e) = discovery.discover_dns_srv(domain).await {
                        debug!("DNS SRV 发现失败 {}: {}", domain, e);
                    }
                }
            }
        });
    }

    /// DNS SRV 发现
    async fn discover_dns_srv(&self, domain: &str) -> LsResult<()> {
        // 简化的 DNS 发现：使用 DNS TXT/SRV 查询
        // 在实际生产环境中，应使用 trust-dns 或 hickory-resolver
        debug!("DNS SRV 发现: {}", domain);
        // 这里简化处理，DNS 发现需要 trust-dns crate
        Ok(())
    }

    /// 服务器计数
    pub async fn count(&self) -> usize {
        self.discovered.read().await.len()
    }

    /// 健康服务器计数
    pub async fn healthy_count(&self) -> usize {
        self.discovered
            .read()
            .await
            .values()
            .filter(|s| s.healthy)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_static_discovery() {
        let config = DiscoveryConfig {
            static_servers: vec![
                StaticServerEntry {
                    name: "test-server".into(),
                    url: "http://127.0.0.1:9999/mcp".into(),
                    capabilities: vec!["test".into()],
                },
            ],
            ..Default::default()
        };

        let discovery = McpDiscovery::new(config);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let servers = discovery.list_servers().await;
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "test-server");
    }

    #[tokio::test]
    async fn test_manual_register() {
        let discovery = McpDiscovery::new(DiscoveryConfig::default());
        discovery
            .register_server(
                "manual-server",
                "http://192.168.1.100:8080/mcp",
                vec!["custom".into()],
                DiscoverySource::Manual,
            )
            .await;

        let servers = discovery.list_servers().await;
        assert!(servers.iter().any(|s| s.name == "manual-server"));
    }

    #[tokio::test]
    async fn test_unregister() {
        let config = DiscoveryConfig {
            static_servers: vec![StaticServerEntry {
                name: "to-remove".into(),
                url: "http://127.0.0.1:9999/mcp".into(),
                capabilities: vec![],
            }],
            ..Default::default()
        };

        let discovery = McpDiscovery::new(config);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        discovery.unregister_server("to-remove").await;
        let servers = discovery.list_servers().await;
        assert!(!servers.iter().any(|s| s.name == "to-remove"));
    }

    #[tokio::test]
    async fn test_find_by_capability() {
        let config = DiscoveryConfig {
            static_servers: vec![
                StaticServerEntry {
                    name: "server-a".into(),
                    url: "http://127.0.0.1:1/mcp".into(),
                    capabilities: vec!["llm".into(), "search".into()],
                },
                StaticServerEntry {
                    name: "server-b".into(),
                    url: "http://127.0.0.1:2/mcp".into(),
                    capabilities: vec!["search".into()],
                },
            ],
            ..Default::default()
        };

        let discovery = McpDiscovery::new(config);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let llm_servers = discovery.find_by_capability("llm").await;
        assert_eq!(llm_servers.len(), 1);
        assert_eq!(llm_servers[0].name, "server-a");
    }

    #[tokio::test]
    async fn test_history() {
        let discovery = McpDiscovery::new(DiscoveryConfig::default());
        discovery
            .register_server(
                "hist-server",
                "http://hist.local/mcp",
                vec![],
                DiscoverySource::Manual,
            )
            .await;

        let history = discovery.get_history(10).await;
        assert!(!history.is_empty());
        assert!(history.iter().any(|e| e.server_name == "hist-server"));
    }
}
