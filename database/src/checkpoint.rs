//! Agent Checkpoint Persistence — Agent 状态持久化存储
//!
//! 提供 SQLite 和 PostgreSQL 两种后端的检查点存储实现。
//! 支持 Agent 状态快照的保存、恢复、版本管理、过期清理。
//!
//! ## Feature Flags
//! - `sqlite` — SQLite 后端 (默认)
//! - `postgres` — PostgreSQL 后端

use async_trait::async_trait;
use lingshu_core::{LsResult, LsError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Agent 检查点.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// 检查点 ID
    pub id: String,
    /// Agent ID
    pub agent_id: String,
    /// 会话 ID
    pub session_id: String,
    /// 序列化的 Agent 状态 (JSON)
    pub state: Value,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 版本号
    pub version: u32,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 过期时间 (None = 永不过期)
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Agent 状态机快照.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Agent ID
    pub agent_id: String,
    /// 当前状态
    pub status: String,
    /// 当前阶段
    pub phase: String,
    /// 状态机数据 (JSON)
    pub state_machine: Value,
    /// 运行时变量
    pub variables: HashMap<String, Value>,
    /// 历史事件
    pub history: Vec<Value>,
    /// 版本号
    pub version: u32,
    /// 更新时间
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 对话嵌入记录.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationEmbedding {
    /// 记录 ID
    pub id: String,
    /// 会话 ID
    pub session_id: String,
    /// 角色 (user/assistant/system)
    pub role: String,
    /// 内容
    pub content: String,
    /// 嵌入向量 (可选, 字节形式)
    pub embedding: Option<Vec<u8>>,
    /// 嵌入模型名称
    pub model: String,
    /// Token 数量
    pub token_count: u32,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Checkpoint 存储查询参数.
#[derive(Debug, Default)]
pub struct CheckpointQuery {
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Checkpoint 存储接口.
#[async_trait]
pub trait CheckpointStore: Send + Sync {
    /// 保存检查点.
    async fn save_checkpoint(&self, cp: &Checkpoint) -> LsResult<()>;

    /// 获取指定 ID 的检查点.
    async fn get_checkpoint(&self, id: &str) -> LsResult<Option<Checkpoint>>;

    /// 查询检查点.
    async fn query_checkpoints(&self, query: &CheckpointQuery) -> LsResult<Vec<Checkpoint>>;

    /// 删除检查点.
    async fn delete_checkpoint(&self, id: &str) -> LsResult<bool>;

    /// 删除 Agent 的所有检查点.
    async fn delete_agent_checkpoints(&self, agent_id: &str) -> LsResult<u64>;

    /// 保存 Agent 状态机.
    async fn save_agent_state(&self, state: &AgentState) -> LsResult<()>;

    /// 获取 Agent 状态机.
    async fn get_agent_state(&self, agent_id: &str) -> LsResult<Option<AgentState>>;

    /// 保存对话嵌入.
    async fn save_embedding(&self, emb: &ConversationEmbedding) -> LsResult<()>;

    /// 查询会话嵌入.
    async fn query_embeddings(&self, session_id: &str) -> LsResult<Vec<ConversationEmbedding>>;

    /// 清理过期的检查点.
    async fn clean_expired(&self) -> LsResult<u64>;
}

// ── SQLite 实现 ────────────────────────────────────

#[cfg(feature = "sqlite")]
pub mod sqlite_store {
    use super::*;
    use rusqlite::params;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    pub struct SqliteCheckpointStore {
        conn: Arc<Mutex<rusqlite::Connection>>,
    }

    impl SqliteCheckpointStore {
        pub fn new(conn: Arc<Mutex<rusqlite::Connection>>) -> Self {
            Self { conn }
        }

        pub fn from_connection(conn: rusqlite::Connection) -> LsResult<Self> {
            Ok(Self {
                conn: Arc::new(Mutex::new(conn)),
            })
        }

        pub fn in_memory() -> LsResult<Self> {
            let conn = rusqlite::Connection::open_in_memory()
                .map_err(|e| LsError::Internal(format!("sqlite in-memory: {e}")))?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS agent_checkpoints (
                    id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, session_id TEXT NOT NULL,
                    state_json TEXT NOT NULL, metadata TEXT NOT NULL DEFAULT '{}',
                    version INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL,
                    expires_at TEXT
                );
                CREATE TABLE IF NOT EXISTS agent_states (
                    id TEXT PRIMARY KEY, agent_id TEXT NOT NULL UNIQUE,
                    status TEXT NOT NULL DEFAULT 'idle', phase TEXT NOT NULL DEFAULT 'init',
                    state_machine_json TEXT NOT NULL DEFAULT '{}',
                    variables TEXT NOT NULL DEFAULT '{}', history TEXT NOT NULL DEFAULT '[]',
                    version INTEGER NOT NULL DEFAULT 1,
                    updated_at TEXT NOT NULL, created_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS conversation_embeddings (
                    id TEXT PRIMARY KEY, session_id TEXT NOT NULL,
                    role TEXT NOT NULL, content TEXT NOT NULL,
                    embedding BLOB, model TEXT NOT NULL DEFAULT 'text-embedding-ada-002',
                    token_count INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL
                );"
            ).map_err(|e| LsError::Internal(format!("sqlite create tables: {e}")))?;
            Ok(Self {
                conn: Arc::new(Mutex::new(conn)),
            })
        }
    }

    #[async_trait]
    impl CheckpointStore for SqliteCheckpointStore {
        async fn save_checkpoint(&self, cp: &Checkpoint) -> LsResult<()> {
            let conn = self.conn.lock().await;
            conn.execute(
                "INSERT OR REPLACE INTO agent_checkpoints
                 (id, agent_id, session_id, state_json, metadata, version, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    cp.id,
                    cp.agent_id,
                    cp.session_id,
                    serde_json::to_string(&cp.state).map_err(|e| LsError::Internal(e.to_string()))?,
                    serde_json::to_string(&cp.metadata).map_err(|e| LsError::Internal(e.to_string()))?,
                    cp.version,
                    cp.created_at.to_rfc3339(),
                    cp.expires_at.map(|d| d.to_rfc3339()),
                ],
            ).map_err(|e| LsError::Internal(format!("save checkpoint: {e}")))?;
            Ok(())
        }

        async fn get_checkpoint(&self, id: &str) -> LsResult<Option<Checkpoint>> {
            let conn = self.conn.lock().await;
            let mut stmt = conn.prepare(
                "SELECT id, agent_id, session_id, state_json, metadata, version, created_at, expires_at
                 FROM agent_checkpoints WHERE id = ?1"
            ).map_err(|e| LsError::Internal(e.to_string()))?;

            let mut rows = stmt.query_map(params![id], |row| {
                Ok(Checkpoint {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    session_id: row.get(2)?,
                    state: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                    metadata: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    version: row.get::<_, i32>(5)? as u32,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                        .map(|d| d.into()).unwrap_or_else(|_| chrono::Utc::now()),
                    expires_at: row.get::<_, Option<String>>(7)?
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|d| d.into())),
                })
            }).map_err(|e| LsError::Internal(e.to_string()))?;

            match rows.next() {
                Some(Ok(cp)) => Ok(Some(cp)),
                _ => Ok(None),
            }
        }

        async fn query_checkpoints(&self, query: &CheckpointQuery) -> LsResult<Vec<Checkpoint>> {
            let conn = self.conn.lock().await;
            let mut sql = "SELECT id, agent_id, session_id, state_json, metadata, version, created_at, expires_at FROM agent_checkpoints WHERE 1=1".to_string();
            let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(ref agent_id) = query.agent_id {
                sql.push_str(" AND agent_id = ?");
                params_vec.push(Box::new(agent_id.clone()));
            }
            if let Some(ref session_id) = query.session_id {
                sql.push_str(" AND session_id = ?");
                params_vec.push(Box::new(session_id.clone()));
            }
            sql.push_str(" ORDER BY created_at DESC");
            if let Some(limit) = query.limit {
                sql.push_str(&format!(" LIMIT {limit}"));
            }
            if let Some(offset) = query.offset {
                sql.push_str(&format!(" OFFSET {offset}"));
            }

            let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&sql).map_err(|e| LsError::Internal(e.to_string()))?;
            let rows = stmt.query_map(param_refs.as_slice(), |row| {
                Ok(Checkpoint {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    session_id: row.get(2)?,
                    state: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                    metadata: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    version: row.get::<_, i32>(5)? as u32,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                        .map(|d| d.into()).unwrap_or_else(|_| chrono::Utc::now()),
                    expires_at: row.get::<_, Option<String>>(7)?
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|d| d.into())),
                })
            }).map_err(|e| LsError::Internal(e.to_string()))?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| LsError::Internal(e.to_string()))?);
            }
            Ok(results)
        }

        async fn delete_checkpoint(&self, id: &str) -> LsResult<bool> {
            let conn = self.conn.lock().await;
            let affected = conn.execute(
                "DELETE FROM agent_checkpoints WHERE id = ?1",
                params![id],
            ).map_err(|e| LsError::Internal(e.to_string()))?;
            Ok(affected > 0)
        }

        async fn delete_agent_checkpoints(&self, agent_id: &str) -> LsResult<u64> {
            let conn = self.conn.lock().await;
            let affected = conn.execute(
                "DELETE FROM agent_checkpoints WHERE agent_id = ?1",
                params![agent_id],
            ).map_err(|e| LsError::Internal(e.to_string()))?;
            Ok(affected as u64)
        }

        async fn save_agent_state(&self, state: &AgentState) -> LsResult<()> {
            let conn = self.conn.lock().await;
            conn.execute(
                "INSERT OR REPLACE INTO agent_states
                 (id, agent_id, status, phase, state_machine_json, variables, history, version, updated_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    format!("state-{}", state.agent_id),
                    state.agent_id,
                    state.status,
                    state.phase,
                    serde_json::to_string(&state.state_machine).map_err(|e| LsError::Internal(e.to_string()))?,
                    serde_json::to_string(&state.variables).map_err(|e| LsError::Internal(e.to_string()))?,
                    serde_json::to_string(&state.history).map_err(|e| LsError::Internal(e.to_string()))?,
                    state.version,
                    state.updated_at.to_rfc3339(),
                    state.created_at.to_rfc3339(),
                ],
            ).map_err(|e| LsError::Internal(format!("save agent state: {e}")))?;
            Ok(())
        }

        async fn get_agent_state(&self, agent_id: &str) -> LsResult<Option<AgentState>> {
            let conn = self.conn.lock().await;
            let mut stmt = conn.prepare(
                "SELECT agent_id, status, phase, state_machine_json, variables, history, version, updated_at, created_at
                 FROM agent_states WHERE agent_id = ?1"
            ).map_err(|e| LsError::Internal(e.to_string()))?;

            let mut rows = stmt.query_map(params![agent_id], |row| {
                Ok(AgentState {
                    agent_id: row.get(0)?,
                    status: row.get(1)?,
                    phase: row.get(2)?,
                    state_machine: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                    variables: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    history: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                    version: row.get::<_, i32>(6)? as u32,
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                        .map(|d| d.into()).unwrap_or_else(|_| chrono::Utc::now()),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                        .map(|d| d.into()).unwrap_or_else(|_| chrono::Utc::now()),
                })
            }).map_err(|e| LsError::Internal(e.to_string()))?;

            match rows.next() {
                Some(Ok(state)) => Ok(Some(state)),
                _ => Ok(None),
            }
        }

        async fn save_embedding(&self, emb: &ConversationEmbedding) -> LsResult<()> {
            let conn = self.conn.lock().await;
            conn.execute(
                "INSERT OR REPLACE INTO conversation_embeddings
                 (id, session_id, role, content, embedding, model, token_count, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    emb.id,
                    emb.session_id,
                    emb.role,
                    emb.content,
                    emb.embedding,
                    emb.model,
                    emb.token_count,
                    emb.created_at.to_rfc3339(),
                ],
            ).map_err(|e| LsError::Internal(format!("save embedding: {e}")))?;
            Ok(())
        }

        async fn query_embeddings(&self, session_id: &str) -> LsResult<Vec<ConversationEmbedding>> {
            let conn = self.conn.lock().await;
            let mut stmt = conn.prepare(
                "SELECT id, session_id, role, content, embedding, model, token_count, created_at
                 FROM conversation_embeddings WHERE session_id = ?1 ORDER BY created_at ASC"
            ).map_err(|e| LsError::Internal(e.to_string()))?;

            let rows = stmt.query_map(params![session_id], |row| {
                Ok(ConversationEmbedding {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    embedding: row.get(4)?,
                    model: row.get(5)?,
                    token_count: row.get::<_, i32>(6)? as u32,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                        .map(|d| d.into()).unwrap_or_else(|_| chrono::Utc::now()),
                })
            }).map_err(|e| LsError::Internal(e.to_string()))?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| LsError::Internal(e.to_string()))?);
            }
            Ok(results)
        }

        async fn clean_expired(&self) -> LsResult<u64> {
            let conn = self.conn.lock().await;
            let now = chrono::Utc::now().to_rfc3339();
            let affected = conn.execute(
                "DELETE FROM agent_checkpoints WHERE expires_at IS NOT NULL AND expires_at < ?1",
                params![now],
            ).map_err(|e| LsError::Internal(format!("clean expired: {e}")))?;
            Ok(affected as u64)
        }
    }
}

#[cfg(test)]
#[cfg(feature = "sqlite")]
mod tests {
    use super::*;
    use super::sqlite_store::SqliteCheckpointStore;

    fn create_store() -> SqliteCheckpointStore {
        SqliteCheckpointStore::in_memory().unwrap()
    }

    fn sample_checkpoint(agent_id: &str) -> Checkpoint {
        let mut metadata = HashMap::new();
        metadata.insert("source".into(), "test".into());
        Checkpoint {
            id: format!("cp-{agent_id}-1"),
            agent_id: agent_id.into(),
            session_id: "session-1".into(),
            state: serde_json::json!({"phase": "planning", "data": {"task": "test"}}),
            metadata,
            version: 1,
            created_at: chrono::Utc::now(),
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn test_save_and_get_checkpoint() {
        let store = create_store();
        let cp = sample_checkpoint("agent-1");
        store.save_checkpoint(&cp).await.unwrap();

        let loaded = store.get_checkpoint("cp-agent-1-1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().agent_id, "agent-1");
    }

    #[tokio::test]
    async fn test_get_nonexistent_checkpoint() {
        let store = create_store();
        let loaded = store.get_checkpoint("nonexistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_delete_checkpoint() {
        let store = create_store();
        let cp = sample_checkpoint("agent-2");
        store.save_checkpoint(&cp).await.unwrap();

        assert!(store.delete_checkpoint("cp-agent-2-1").await.unwrap());
        assert!(store.get_checkpoint("cp-agent-2-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let store = create_store();
        assert!(!store.delete_checkpoint("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_query_by_agent() {
        let store = create_store();
        store.save_checkpoint(&sample_checkpoint("agent-a")).await.unwrap();
        store.save_checkpoint(&sample_checkpoint("agent-b")).await.unwrap();

        let query = CheckpointQuery {
            agent_id: Some("agent-a".into()),
            ..Default::default()
        };
        let results = store.query_checkpoints(&query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_id, "agent-a");
    }

    #[tokio::test]
    async fn test_delete_agent_checkpoints() {
        let store = create_store();
        for i in 0..3 {
            let mut cp = sample_checkpoint("agent-x");
            cp.id = format!("cp-agent-x-{i}");
            store.save_checkpoint(&cp).await.unwrap();
        }
        let deleted = store.delete_agent_checkpoints("agent-x").await.unwrap();
        assert_eq!(deleted, 3);
    }

    #[tokio::test]
    async fn test_save_and_get_agent_state() {
        let store = create_store();
        let state = AgentState {
            agent_id: "agent-1".into(),
            status: "running".into(),
            phase: "execute".into(),
            state_machine: serde_json::json!({"current": "step_3"}),
            variables: [("key".into(), serde_json::json!("value"))].into(),
            history: vec![serde_json::json!({"event": "started"})],
            version: 1,
            updated_at: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
        };
        store.save_agent_state(&state).await.unwrap();

        let loaded = store.get_agent_state("agent-1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().status, "running");
    }

    #[tokio::test]
    async fn test_save_embedding() {
        let store = create_store();
        let emb = ConversationEmbedding {
            id: "emb-1".into(),
            session_id: "sess-1".into(),
            role: "user".into(),
            content: "hello world".into(),
            embedding: Some(vec![0.1f64, 0.2f64, 0.3f64].into_iter().flat_map(f64::to_le_bytes).collect()),
            model: "test-model".into(),
            token_count: 5,
            created_at: chrono::Utc::now(),
        };
        store.save_embedding(&emb).await.unwrap();

        let results = store.query_embeddings("sess-1").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "hello world");
    }

    #[tokio::test]
    async fn test_clean_expired() {
        let store = create_store();
        let mut cp = sample_checkpoint("expired-agent");
        cp.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        store.save_checkpoint(&cp).await.unwrap();

        let cleaned = store.clean_expired().await.unwrap();
        assert_eq!(cleaned, 1);
    }
}
