//! LSFed — 跨集群状态复制.
//!
//! 将本地集群的状态（内存、键值、Agent 状态等）复制到远端集群。
//!
//! ## 复制策略
//!
//! - 广播 (Broadcast) — 复制到所有已连接集群
//! - 定向 (Direct) — 复制到指定集群
//! - 按命名空间过滤 — 只复制特定 namespace 的状态

use crate::link::LinkManager;
use crate::protocol::{FederationMessage, StateReplicateAckPayload, StateReplicatePayload};
use lingshu_core::LsResult;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// 复制策略.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationStrategy {
    /// 广播到所有节点.
    Broadcast,
    /// 只复制到 leader.
    ToLeader,
    /// 复制到指定节点.
    Direct,
}

/// 状态存储后端.
#[async_trait::async_trait]
pub trait StateBackend: Send + Sync {
    /// 读取状态.
    async fn get(&self, namespace: &str, key: &str) -> Option<Value>;

    /// 写入状态.
    async fn set(&self, namespace: &str, key: &str, value: Value) -> LsResult<()>;

    /// 列出命名空间下所有键.
    async fn list_keys(&self, namespace: &str) -> Vec<String>;
}

/// 内存状态后端.
pub struct MemoryStateBackend {
    data: Arc<RwLock<HashMap<String, HashMap<String, (Value, u64)>>>>,
}

impl MemoryStateBackend {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl StateBackend for MemoryStateBackend {
    async fn get(&self, namespace: &str, key: &str) -> Option<Value> {
        let data = self.data.read().await;
        data.get(namespace)
            .and_then(|ns| ns.get(key))
            .map(|(v, _)| v.clone())
    }

    async fn set(&self, namespace: &str, key: &str, value: Value) -> LsResult<()> {
        let mut data = self.data.write().await;
        let ns = data.entry(namespace.to_string()).or_default();
        let version = ns.get(key).map(|(_, v)| v + 1).unwrap_or(1);
        ns.insert(key.to_string(), (value, version));
        Ok(())
    }

    async fn list_keys(&self, namespace: &str) -> Vec<String> {
        let data = self.data.read().await;
        data.get(namespace)
            .map(|ns| ns.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// 状态复制器.
pub struct StateReplicator {
    /// 连接管理器.
    link_mgr: Arc<LinkManager>,
    /// 状态后端.
    backend: Arc<dyn StateBackend>,
    /// 复制策略.
    strategy: ReplicationStrategy,
    /// 要复制的命名空间列表（空 = 全部）.
    replicated_namespaces: Arc<RwLock<Vec<String>>>,
}

impl StateReplicator {
    pub fn new(link_mgr: Arc<LinkManager>, backend: Arc<dyn StateBackend>) -> Self {
        Self {
            link_mgr,
            backend,
            strategy: ReplicationStrategy::Broadcast,
            replicated_namespaces: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 设置复制策略.
    pub fn set_strategy(&mut self, strategy: ReplicationStrategy) {
        self.strategy = strategy;
    }

    /// 添加要复制的命名空间.
    pub async fn add_namespace(&self, namespace: &str) {
        self.replicated_namespaces
            .write()
            .await
            .push(namespace.to_string());
    }

    /// 写状态并同步复制到远端.
    pub async fn set(&self, namespace: &str, key: &str, value: Value) -> LsResult<()> {
        self.backend.set(namespace, key, value.clone()).await?;

        // 检查是否需要复制
        let should_replicate = {
            let namespaces = self.replicated_namespaces.read().await;
            namespaces.is_empty() || namespaces.iter().any(|ns| ns == namespace)
        };

        if should_replicate {
            let version = self
                .backend
                .get(namespace, key)
                .await
                .and_then(|_| {
                    // 从后端获取版本（简化：使用时间戳）
                    Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    )
                })
                .unwrap_or(0);

            let payload = StateReplicatePayload {
                key: key.to_string(),
                value,
                version,
                namespace: namespace.to_string(),
            };

            let msg = FederationMessage::StateReplicate(payload);
            match self.strategy {
                ReplicationStrategy::Broadcast => {
                    self.link_mgr.broadcast(msg);
                }
                ReplicationStrategy::ToLeader => {
                    // 单播到 leader（简化：广播）
                    self.link_mgr.broadcast(msg);
                }
                ReplicationStrategy::Direct => {
                    // 定向由调用者处理
                }
            }
        }

        Ok(())
    }

    /// 读取状态.
    pub async fn get(&self, namespace: &str, key: &str) -> Option<Value> {
        self.backend.get(namespace, key).await
    }

    /// 处理入站的状态复制消息.
    pub async fn handle_replicate(
        &self,
        payload: StateReplicatePayload,
    ) -> StateReplicateAckPayload {
        let result = self
            .backend
            .set(&payload.namespace, &payload.key, payload.value)
            .await;

        match result {
            Ok(()) => {
                debug!(
                    namespace = %payload.namespace,
                    key = %payload.key,
                    version = payload.version,
                    "state replicated"
                );
                StateReplicateAckPayload {
                    key: payload.key,
                    version: payload.version,
                    accepted: true,
                    error: None,
                }
            }
            Err(e) => {
                warn!(
                    namespace = %payload.namespace,
                    key = %payload.key,
                    error = %e,
                    "state replication failed"
                );
                StateReplicateAckPayload {
                    key: payload.key,
                    version: payload.version,
                    accepted: false,
                    error: Some(e.to_string()),
                }
            }
        }
    }

    /// 列出命名空间中所有键.
    pub async fn list_keys(&self, namespace: &str) -> Vec<String> {
        self.backend.list_keys(namespace).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FederationConfig;

    #[tokio::test]
    async fn test_memory_backend() {
        let backend = MemoryStateBackend::new();
        backend
            .set("agents", "agent-1", serde_json::json!({"name": "test"}))
            .await
            .unwrap();
        let val = backend.get("agents", "agent-1").await;
        assert!(val.is_some());
        assert_eq!(val.unwrap()["name"], "test");
    }

    #[tokio::test]
    async fn test_replicator_set_get() {
        let link_mgr = Arc::new(LinkManager::new(
            lingshu_core::LsId::new(),
            "test",
            FederationConfig::default(),
        ));
        let backend = Arc::new(MemoryStateBackend::new());
        let replicator = StateReplicator::new(link_mgr, backend);

        replicator
            .set("test-ns", "test-key", serde_json::json!("test-value"))
            .await
            .unwrap();

        let val = replicator.get("test-ns", "test-key").await;
        assert_eq!(val, Some(serde_json::json!("test-value")));
    }

    #[tokio::test]
    async fn test_handle_replicate() {
        let link_mgr = Arc::new(LinkManager::new(
            lingshu_core::LsId::new(),
            "test",
            FederationConfig::default(),
        ));
        let backend = Arc::new(MemoryStateBackend::new());
        let replicator = StateReplicator::new(link_mgr, backend);

        let payload = StateReplicatePayload {
            key: "remote-key".into(),
            value: serde_json::json!("remote-value"),
            version: 1,
            namespace: "remote".into(),
        };

        let ack = replicator.handle_replicate(payload).await;
        assert!(ack.accepted);
        assert_eq!(ack.key, "remote-key");

        let val = replicator.get("remote", "remote-key").await;
        assert_eq!(val, Some(serde_json::json!("remote-value")));
    }
}
