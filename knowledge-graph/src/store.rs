//! GraphStore — 知识图谱持久化存储 (SQLite).
//!
//! 将 `KnowledgeGraph` 序列化为 JSON 存入 SQLite，支持重启恢复。

use std::path::Path;
use std::sync::Arc;

use lingshu_core::{LsError, LsResult};
use rusqlite::params;
use tokio::sync::Mutex;

use crate::KnowledgeGraph;

/// 图谱持久化存储.
///
/// 使用 SQLite 存储序列化后的 `KnowledgeGraph`。
/// 每个 project 对应一行，以 project name 为主键。
pub struct GraphStore {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl GraphStore {
    /// 打开或创建 SQLite 存储文件.
    pub fn open(path: impl AsRef<Path>) -> LsResult<Self> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| LsError::Internal(format!("graph store open failed: {e}")))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;",
        )
        .map_err(|e| LsError::Internal(format!("graph store pragma failed: {e}")))?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 创建内存存储 (测试用).
    pub fn in_memory() -> LsResult<Self> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|e| LsError::Internal(format!("graph store in-memory open: {e}")))?;
        Self::run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &rusqlite::Connection) -> LsResult<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS graph_cache (
                project_name TEXT PRIMARY KEY NOT NULL,
                graph_json   TEXT NOT NULL,
                node_count   INTEGER NOT NULL DEFAULT 0,
                edge_count   INTEGER NOT NULL DEFAULT 0,
                created_at   TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .map_err(|e| LsError::Internal(format!("graph store migration failed: {e}")))?;
        tracing::info!("graph store: migrations applied");
        Ok(())
    }

    /// 保存图谱到存储.
    pub async fn save(&self, project: &str, graph: &KnowledgeGraph) -> LsResult<()> {
        let json = serde_json::to_string(graph)
            .map_err(|e| LsError::Internal(format!("serialize graph: {e}")))?;
        let node_count = graph.nodes.len() as i64;
        let edge_count = graph.edges.len() as i64;

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO graph_cache (project_name, graph_json, node_count, edge_count, created_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            params![project, json, node_count, edge_count],
        )
        .map_err(|e| LsError::Internal(format!("graph store insert: {e}")))?;

        tracing::info!(project = %project, nodes = node_count, edges = edge_count, "graph saved to store");
        Ok(())
    }

    /// 从存储加载指定项目的图谱.
    pub async fn load(&self, project: &str) -> LsResult<Option<KnowledgeGraph>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare("SELECT graph_json FROM graph_cache WHERE project_name = ?1")
            .map_err(|e| LsError::Internal(format!("graph store prepare: {e}")))?;

        let result = stmt
            .query_row(params![project], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .ok();

        match result {
            Some(json) => {
                let graph: KnowledgeGraph = serde_json::from_str(&json)
                    .map_err(|e| LsError::Internal(format!("deserialize graph: {e}")))?;
                Ok(Some(graph))
            }
            None => Ok(None),
        }
    }

    /// 加载所有已缓存的项目名称.
    pub async fn list_projects(&self) -> LsResult<Vec<String>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare("SELECT project_name FROM graph_cache ORDER BY created_at DESC")
            .map_err(|e| LsError::Internal(format!("graph store prepare: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                Ok(name)
            })
            .map_err(|e| LsError::Internal(format!("graph store query: {e}")))?;

        let mut projects = Vec::new();
        for row in rows {
            if let Ok(name) = row {
                projects.push(name);
            }
        }
        Ok(projects)
    }

    /// 加载所有图谱到内存 (启动时恢复用).
    pub async fn load_all(&self) -> LsResult<std::collections::HashMap<String, KnowledgeGraph>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare("SELECT project_name, graph_json FROM graph_cache ORDER BY created_at DESC")
            .map_err(|e| LsError::Internal(format!("graph store prepare: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                let json: String = row.get(1)?;
                Ok((name, json))
            })
            .map_err(|e| LsError::Internal(format!("graph store query: {e}")))?;

        let mut map = std::collections::HashMap::new();
        for row in rows {
            if let Ok((name, json)) = row {
                if let Ok(graph) = serde_json::from_str::<KnowledgeGraph>(&json) {
                    map.insert(name, graph);
                } else {
                    tracing::warn!(project = %name, "failed to deserialize cached graph, skipping");
                }
            }
        }
        tracing::info!(count = map.len(), "restored graphs from store");
        Ok(map)
    }

    /// 删除指定项目的缓存.
    pub async fn delete(&self, project: &str) -> LsResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM graph_cache WHERE project_name = ?1",
            params![project],
        )
        .map_err(|e| LsError::Internal(format!("graph store delete: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KnowledgeGraph;

    #[tokio::test]
    async fn test_save_and_load() {
        let store = GraphStore::in_memory().unwrap();
        let graph = KnowledgeGraph::new("test-project", "abc123");

        store.save("test-project", &graph).await.unwrap();

        let loaded = store.load("test-project").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().project.name, "test-project");
    }

    #[tokio::test]
    async fn test_load_nonexistent() {
        let store = GraphStore::in_memory().unwrap();
        let result = store.load("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_projects() {
        let store = GraphStore::in_memory().unwrap();
        store
            .save("a", &KnowledgeGraph::new("a", ""))
            .await
            .unwrap();
        store
            .save("b", &KnowledgeGraph::new("b", ""))
            .await
            .unwrap();

        let projects = store.list_projects().await.unwrap();
        assert!(projects.contains(&"a".to_string()));
        assert!(projects.contains(&"b".to_string()));
    }

    #[tokio::test]
    async fn test_delete() {
        let store = GraphStore::in_memory().unwrap();
        store
            .save("x", &KnowledgeGraph::new("x", ""))
            .await
            .unwrap();
        assert!(store.load("x").await.unwrap().is_some());

        store.delete("x").await.unwrap();
        assert!(store.load("x").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_load_all() {
        let store = GraphStore::in_memory().unwrap();
        store
            .save("p1", &KnowledgeGraph::new("p1", ""))
            .await
            .unwrap();
        store
            .save("p2", &KnowledgeGraph::new("p2", ""))
            .await
            .unwrap();

        let all = store.load_all().await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("p1"));
        assert!(all.contains_key("p2"));
    }
}
