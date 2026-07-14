//! AgentPersistence — Agent 状态持久化.
//!
//! 支持将 Agent 的会话状态、对话历史和快照保存到存储后端，
//! 以便在系统重启或 Agent 迁移后恢复执行。

use async_trait::async_trait;
use lingshu_core::{LsError, LsId, LsResult};
use lingshu_traits::agent::{AgentSnapshot, AgentStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Agent 记录 — 持久化存储的 Agent 信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    /// Agent ID.
    pub agent_id: LsId,
    /// Agent 名称.
    pub name: String,
    /// Agent 状态.
    pub status: AgentStatus,
    /// 会话 ID.
    pub session_id: LsId,
    /// 对话历史 (序列化的 LLM 消息).
    pub messages: Option<Vec<u8>>,
    /// 额外元数据.
    pub metadata: HashMap<String, String>,
    /// 创建时间.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 最后活跃时间.
    pub last_active_at: chrono::DateTime<chrono::Utc>,
}

/// Agent 持久化存储接口.
#[async_trait]
pub trait AgentStore: Send + Sync {
    /// 保存 Agent 记录.
    async fn save(&self, record: &AgentRecord) -> LsResult<()>;

    /// 根据 ID 加载 Agent 记录.
    async fn load(&self, agent_id: &LsId) -> LsResult<Option<AgentRecord>>;

    /// 删除 Agent 记录.
    async fn delete(&self, agent_id: &LsId) -> LsResult<()>;

    /// 列出所有 Agent 记录.
    async fn list(&self) -> LsResult<Vec<AgentRecord>>;

    /// 按会话 ID 查询 Agent 记录.
    async fn list_by_session(&self, session_id: &LsId) -> LsResult<Vec<AgentRecord>>;

    /// 更新 Agent 状态.
    async fn update_status(&self, agent_id: &LsId, status: AgentStatus) -> LsResult<()>;
}

/// 内存 Agent 存储（用于测试和单进程场景）.
pub struct InMemoryAgentStore {
    agents: Arc<RwLock<HashMap<LsId, AgentRecord>>>,
}

impl InMemoryAgentStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for InMemoryAgentStore {
    fn default() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl AgentStore for InMemoryAgentStore {
    async fn save(&self, record: &AgentRecord) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        agents.insert(record.agent_id, record.clone());
        debug!(agent_id = %record.agent_id, "agent record saved in memory");
        Ok(())
    }

    async fn load(&self, agent_id: &LsId) -> LsResult<Option<AgentRecord>> {
        let agents = self.agents.read().await;
        Ok(agents.get(agent_id).cloned())
    }

    async fn delete(&self, agent_id: &LsId) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        agents.remove(agent_id);
        Ok(())
    }

    async fn list(&self) -> LsResult<Vec<AgentRecord>> {
        let agents = self.agents.read().await;
        Ok(agents.values().cloned().collect())
    }

    async fn list_by_session(&self, session_id: &LsId) -> LsResult<Vec<AgentRecord>> {
        let agents = self.agents.read().await;
        Ok(agents
            .values()
            .filter(|r| r.session_id == *session_id)
            .cloned()
            .collect())
    }

    async fn update_status(&self, agent_id: &LsId, status: AgentStatus) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        if let Some(record) = agents.get_mut(agent_id) {
            record.status = status;
            record.last_active_at = chrono::Utc::now();
            Ok(())
        } else {
            Err(LsError::NotFound(format!("agent {agent_id}")))
        }
    }
}

/// Agent 持久化管理器.
pub struct AgentPersistenceManager {
    store: Arc<dyn AgentStore>,
}

impl AgentPersistenceManager {
    pub fn new(store: Arc<dyn AgentStore>) -> Self {
        Self { store }
    }

    /// 保存 Agent 记录.
    pub async fn save_agent(
        &self,
        agent_id: LsId,
        name: String,
        session_id: LsId,
        messages: Option<&[u8]>,
        metadata: HashMap<String, String>,
    ) -> LsResult<AgentRecord> {
        let record = AgentRecord {
            agent_id,
            name,
            status: AgentStatus::Idle,
            session_id,
            messages: messages.map(|m| m.to_vec()),
            metadata,
            created_at: chrono::Utc::now(),
            last_active_at: chrono::Utc::now(),
        };
        self.store.save(&record).await?;
        Ok(record)
    }

    /// 加载 Agent 记录.
    pub async fn load_agent(&self, agent_id: &LsId) -> LsResult<Option<AgentRecord>> {
        self.store.load(agent_id).await
    }

    /// 删除 Agent 记录.
    pub async fn delete_agent(&self, agent_id: &LsId) -> LsResult<()> {
        self.store.delete(agent_id).await
    }

    /// 列出所有 Agent.
    pub async fn list_agents(&self) -> LsResult<Vec<AgentRecord>> {
        self.store.list().await
    }

    /// 按会话列出 Agent.
    pub async fn list_agents_by_session(&self, session_id: &LsId) -> LsResult<Vec<AgentRecord>> {
        self.store.list_by_session(session_id).await
    }

    /// 更新 Agent 状态.
    pub async fn update_status(&self, agent_id: &LsId, status: AgentStatus) -> LsResult<()> {
        self.store.update_status(agent_id, status).await
    }

    /// 从快照恢复 Agent 记录.
    pub async fn save_from_snapshot(
        &self,
        snapshot: &AgentSnapshot,
        name: &str,
        session_id: &LsId,
    ) -> LsResult<AgentRecord> {
        let record = AgentRecord {
            agent_id: snapshot.agent_id,
            name: name.to_string(),
            status: snapshot.status,
            session_id: *session_id,
            messages: Some(snapshot.state.clone()),
            metadata: HashMap::new(),
            created_at: snapshot.created_at,
            last_active_at: chrono::Utc::now(),
        };
        self.store.save(&record).await?;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_save_and_load() {
        let store = Arc::new(InMemoryAgentStore::new()) as Arc<dyn AgentStore>;
        let mgr = AgentPersistenceManager::new(store);

        let agent_id = LsId::new();
        let session_id = LsId::new();

        mgr.save_agent(
            agent_id,
            "test-agent".into(),
            session_id,
            None,
            HashMap::new(),
        )
        .await
        .unwrap();

        let loaded = mgr.load_agent(&agent_id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name, "test-agent");
    }

    #[tokio::test]
    async fn test_load_not_found() {
        let store = Arc::new(InMemoryAgentStore::new()) as Arc<dyn AgentStore>;
        let mgr = AgentPersistenceManager::new(store);

        let loaded = mgr.load_agent(&LsId::new()).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_delete() {
        let store = Arc::new(InMemoryAgentStore::new()) as Arc<dyn AgentStore>;
        let mgr = AgentPersistenceManager::new(store);

        let agent_id = LsId::new();
        mgr.save_agent(agent_id, "test".into(), LsId::new(), None, HashMap::new())
            .await
            .unwrap();

        mgr.delete_agent(&agent_id).await.unwrap();
        let loaded = mgr.load_agent(&agent_id).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_list_by_session() {
        let store = Arc::new(InMemoryAgentStore::new()) as Arc<dyn AgentStore>;
        let mgr = AgentPersistenceManager::new(store);

        let session_id = LsId::new();
        let agent_a = LsId::new();
        let agent_b = LsId::new();
        let other_session = LsId::new();

        mgr.save_agent(agent_a, "a".into(), session_id, None, HashMap::new())
            .await
            .unwrap();
        mgr.save_agent(agent_b, "b".into(), session_id, None, HashMap::new())
            .await
            .unwrap();
        mgr.save_agent(LsId::new(), "c".into(), other_session, None, HashMap::new())
            .await
            .unwrap();

        let agents = mgr.list_agents_by_session(&session_id).await.unwrap();
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_update_status() {
        let store = Arc::new(InMemoryAgentStore::new()) as Arc<dyn AgentStore>;
        let mgr = AgentPersistenceManager::new(store);

        let agent_id = LsId::new();
        mgr.save_agent(agent_id, "test".into(), LsId::new(), None, HashMap::new())
            .await
            .unwrap();

        mgr.update_status(&agent_id, AgentStatus::Running)
            .await
            .unwrap();
        let loaded = mgr.load_agent(&agent_id).await.unwrap().unwrap();
        assert_eq!(loaded.status, AgentStatus::Running);
    }

    #[tokio::test]
    async fn test_list_all() {
        let store = Arc::new(InMemoryAgentStore::new()) as Arc<dyn AgentStore>;
        let mgr = AgentPersistenceManager::new(store);

        mgr.save_agent(LsId::new(), "a".into(), LsId::new(), None, HashMap::new())
            .await
            .unwrap();
        mgr.save_agent(LsId::new(), "b".into(), LsId::new(), None, HashMap::new())
            .await
            .unwrap();

        let list = mgr.list_agents().await.unwrap();
        assert_eq!(list.len(), 2);
    }
}
