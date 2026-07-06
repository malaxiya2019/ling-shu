//! AgentRegistry — 基于能力的智能体注册与发现.
//!
//! 支持按能力 (capability)、状态、名称匹配查询，
//! 提供健康探测和筛选功能。

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::agent::AgentStatus;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;
use tracing::info;

/// 智能体能力声明.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct AgentCapability {
    /// 能力名称, 如 "text-generation", "code-execution", "web-search"
    pub name: String,
    /// 能力版本
    pub version: String,
    /// 可选描述
    pub description: Option<String>,
}

/// 注册的智能体信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_id: LsId,
    pub name: String,
    pub version: String,
    pub capabilities: Vec<AgentCapability>,
    pub status: AgentStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    /// 元数据标签 (key=value)
    pub tags: HashMap<String, String>,
}

/// 健康探测结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub agent_id: LsId,
    pub alive: bool,
    pub status: AgentStatus,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// AgentProbe — 对单个智能体发起健康探测.
#[async_trait::async_trait]
pub trait AgentProbe: Send + Sync + 'static {
    async fn probe(&self, agent_id: &LsId, ctx: &LsContext) -> LsResult<ProbeResult>;
}

/// AgentRegistry — 线程安全的智能体注册表.
pub struct AgentRegistry {
    agents: RwLock<HashMap<LsId, AgentInfo>>,
    /// 能力 -> 拥有该能力的智能体 ID 集合
    capability_index: RwLock<HashMap<String, HashSet<LsId>>>,
}

impl AgentRegistry {
    /// 创建空的注册表.
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            capability_index: RwLock::new(HashMap::new()),
        }
    }

    /// 注册一个智能体.
    pub async fn register(&self, info: AgentInfo) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let agent_id = info.agent_id;
        if agents.contains_key(&agent_id) {
            return Err(LsError::AlreadyExists(format!("agent {agent_id}")));
        }

        // 更新能力索引
        {
            let mut idx = self.capability_index.write().await;
            for cap in &info.capabilities {
                idx.entry(cap.name.clone()).or_default().insert(agent_id);
            }
        }

        agents.insert(agent_id, info);
        info!(agent_id = %agent_id, "agent registered in orchestrator");
        Ok(())
    }

    /// 注销一个智能体.
    pub async fn unregister(&self, agent_id: &LsId) -> LsResult<AgentInfo> {
        let mut agents = self.agents.write().await;
        let info = agents
            .remove(agent_id)
            .ok_or_else(|| LsError::NotFound(format!("agent {agent_id}")))?;

        // 清理能力索引
        {
            let mut idx = self.capability_index.write().await;
            for cap in &info.capabilities {
                if let Some(set) = idx.get_mut(&cap.name) {
                    set.remove(agent_id);
                    if set.is_empty() {
                        idx.remove(&cap.name);
                    }
                }
            }
        }

        info!(agent_id = %agent_id, "agent unregistered");
        Ok(info)
    }

    /// 按 ID 查询.
    pub async fn get(&self, agent_id: &LsId) -> LsResult<AgentInfo> {
        let agents = self.agents.read().await;
        agents
            .get(agent_id)
            .cloned()
            .ok_or_else(|| LsError::NotFound(format!("agent {agent_id}")))
    }

    /// 按能力查找智能体.
    pub async fn find_by_capability(&self, capability: &str) -> Vec<AgentInfo> {
        let agents = self.agents.read().await;
        let idx = self.capability_index.read().await;

        match idx.get(capability) {
            Some(ids) => ids
                .iter()
                .filter_map(|id| agents.get(id).cloned())
                .collect(),
            None => Vec::new(),
        }
    }

    /// 按标签筛选.
    pub async fn find_by_tag(&self, key: &str, value: &str) -> Vec<AgentInfo> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|a| a.tags.get(key).map_or(false, |v| v == value))
            .cloned()
            .collect()
    }

    /// 列出所有注册的智能体.
    pub async fn list(&self) -> Vec<AgentInfo> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// 列出指定状态的智能体.
    pub async fn list_by_status(&self, status: AgentStatus) -> Vec<AgentInfo> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|a| a.status == status)
            .cloned()
            .collect()
    }

    /// 更新智能体状态.
    pub async fn update_status(&self, agent_id: &LsId, status: AgentStatus) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents
            .get_mut(agent_id)
            .ok_or_else(|| LsError::NotFound(format!("agent {agent_id}")))?;
        entry.status = status;
        entry.last_heartbeat = chrono::Utc::now();
        Ok(())
    }

    /// 更新智能体标签.
    pub async fn update_tags(
        &self,
        agent_id: &LsId,
        tags: HashMap<String, String>,
    ) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents
            .get_mut(agent_id)
            .ok_or_else(|| LsError::NotFound(format!("agent {agent_id}")))?;
        entry.tags = tags;
        Ok(())
    }

    /// 添加单个标签.
    pub async fn add_tag(&self, agent_id: &LsId, key: String, value: String) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents
            .get_mut(agent_id)
            .ok_or_else(|| LsError::NotFound(format!("agent {agent_id}")))?;
        entry.tags.insert(key, value);
        Ok(())
    }

    /// 心跳更新.
    pub async fn heartbeat(&self, agent_id: &LsId) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents
            .get_mut(agent_id)
            .ok_or_else(|| LsError::NotFound(format!("agent {agent_id}")))?;
        entry.last_heartbeat = chrono::Utc::now();
        Ok(())
    }

    /// 注册的智能体数量.
    pub async fn count(&self) -> usize {
        self.agents.read().await.len()
    }

    /// 列出所有可用能力.
    pub async fn capabilities(&self) -> Vec<String> {
        self.capability_index.read().await.keys().cloned().collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
pub fn make_info(id: LsId, name: &str, capabilities: Vec<&str>) -> AgentInfo {
    AgentInfo {
        agent_id: id,
        name: name.to_string(),
        version: "1.0".into(),
        capabilities: capabilities
            .into_iter()
            .map(|c| AgentCapability {
                name: c.to_string(),
                version: "1.0".into(),
                description: None,
            })
            .collect(),
        status: AgentStatus::Idle,
        created_at: chrono::Utc::now(),
        last_heartbeat: chrono::Utc::now(),
        tags: std::collections::HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_get() {
        let reg = AgentRegistry::new();
        let id = LsId::new();
        let info = make_info(id, "agent-a", vec!["text-gen", "code"]);
        reg.register(info.clone()).await.unwrap();
        assert_eq!(reg.count().await, 1);
        let fetched = reg.get(&id).await.unwrap();
        assert_eq!(fetched.name, "agent-a");
    }

    #[tokio::test]
    async fn test_duplicate_register() {
        let reg = AgentRegistry::new();
        let id = LsId::new();
        let info = make_info(id, "agent-a", vec!["text-gen"]);
        reg.register(info).await.unwrap();
        let dup = make_info(id, "agent-a-dup", vec!["text-gen"]);
        assert!(reg.register(dup).await.is_err());
    }

    #[tokio::test]
    async fn test_find_by_capability() {
        let reg = AgentRegistry::new();
        let id1 = LsId::new();
        let id2 = LsId::new();
        reg.register(make_info(id1, "coder", vec!["code", "text-gen"]))
            .await
            .unwrap();
        reg.register(make_info(id2, "writer", vec!["text-gen"]))
            .await
            .unwrap();

        let coders = reg.find_by_capability("code").await;
        assert_eq!(coders.len(), 1);
        assert_eq!(coders[0].name, "coder");

        let text_agents = reg.find_by_capability("text-gen").await;
        assert_eq!(text_agents.len(), 2);
    }

    #[tokio::test]
    async fn test_unregister() {
        let reg = AgentRegistry::new();
        let id = LsId::new();
        reg.register(make_info(id, "agent", vec!["test"]))
            .await
            .unwrap();
        reg.unregister(&id).await.unwrap();
        assert_eq!(reg.count().await, 0);
        assert!(reg.get(&id).await.is_err());
    }

    #[tokio::test]
    async fn test_find_by_tag() {
        let reg = AgentRegistry::new();
        let id = LsId::new();
        let mut info = make_info(id, "tagged", vec!["test"]);
        info.tags.insert("env".into(), "dev".into());
        reg.register(info).await.unwrap();

        let found = reg.find_by_tag("env", "dev").await;
        assert_eq!(found.len(), 1);

        let not_found = reg.find_by_tag("env", "prod").await;
        assert_eq!(not_found.len(), 0);
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let reg = AgentRegistry::new();
        let id = LsId::new();
        reg.register(make_info(id, "agent", vec!["test"]))
            .await
            .unwrap();

        let before = reg.get(&id).await.unwrap().last_heartbeat;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        reg.heartbeat(&id).await.unwrap();
        let after = reg.get(&id).await.unwrap().last_heartbeat;
        assert!(after > before);
    }

    #[tokio::test]
    async fn test_capabilities_list() {
        let reg = AgentRegistry::new();
        reg.register(make_info(LsId::new(), "a", vec!["x", "y"]))
            .await
            .unwrap();
        reg.register(make_info(LsId::new(), "b", vec!["y", "z"]))
            .await
            .unwrap();
        let mut caps = reg.capabilities().await;
        caps.sort();
        assert_eq!(caps, vec!["x", "y", "z"]);
    }
}
