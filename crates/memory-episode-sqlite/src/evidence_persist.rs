//! EvidenceGraph 持久化 — 存储和加载 EvidenceGraph。
//!
//! 将 EvidenceGraph 序列化为 JSON 存储在 SQLite 中。
//! 使用独立的 evidence_graphs 表。

use lingshu_core::LsResult;
use lingshu_evidence_graph::EvidenceGraph;
use rusqlite::{params, Connection};

/// 保存 EvidenceGraph 到数据库。
///
/// 自动创建表（如果不存在）。
pub fn save_graph(conn: &Connection, name: &str, graph: &EvidenceGraph) -> LsResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS evidence_graphs (
            name        TEXT PRIMARY KEY NOT NULL,
            graph_json  TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| lingshu_core::LsError::Internal(format!("create evidence_graphs table: {e}")))?;

    let json = serde_json::to_string(graph)
        .map_err(|e| lingshu_core::LsError::Internal(format!("serialize graph: {e}")))?;

    conn.execute(
        "INSERT OR REPLACE INTO evidence_graphs (name, graph_json, updated_at)
         VALUES (?1, ?2, datetime('now'))",
        params![name, json],
    )
    .map_err(|e| lingshu_core::LsError::Internal(format!("save graph: {e}")))?;

    Ok(())
}

/// 从数据库加载 EvidenceGraph。
pub fn load_graph(conn: &Connection, name: &str) -> LsResult<Option<EvidenceGraph>> {
    // 确保表存在
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS evidence_graphs (
            name        TEXT PRIMARY KEY NOT NULL,
            graph_json  TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .ok();

    let json: Option<String> = conn
        .query_row(
            "SELECT graph_json FROM evidence_graphs WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .ok();

    match json {
        Some(j) => {
            let graph: EvidenceGraph = serde_json::from_str(&j)
                .map_err(|e| lingshu_core::LsError::Internal(format!("deserialize graph: {e}")))?;
            Ok(Some(graph))
        }
        None => Ok(None),
    }
}

/// 删除 EvidenceGraph。
pub fn delete_graph(conn: &Connection, name: &str) -> LsResult<bool> {
    let affected = conn
        .execute("DELETE FROM evidence_graphs WHERE name = ?1", params![name])
        .map_err(|e| lingshu_core::LsError::Internal(format!("delete graph: {e}")))?;
    Ok(affected > 0)
}

/// 列出所有已保存的 EvidenceGraph 名称。
pub fn list_graphs(conn: &Connection) -> LsResult<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT name FROM evidence_graphs ORDER BY name")
        .map_err(|e| lingshu_core::LsError::Internal(format!("list graphs: {e}")))?;

    let names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| lingshu_core::LsError::Internal(format!("query graphs: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_evidence_graph::{EvidenceGraph, Node, Edge};

    fn make_graph() -> EvidenceGraph {
        let mut g = EvidenceGraph::empty("test_graph");
        let n1 = Node::event("事件1", "第一个事件", chrono::Utc::now());
        let n2 = Node::event("事件2", "第二个事件", chrono::Utc::now());
        let n1_id = n1.id;
        let n2_id = n2.id;
        g.add_node(n1);
        g.add_node(n2);
        g.add_edge(Edge::temporal(n1_id, n2_id));
        g
    }

    #[test]
    fn test_save_and_load_graph() {
        let conn = Connection::open_in_memory().unwrap();
        let graph = make_graph();

        save_graph(&conn, "test", &graph).unwrap();
        let loaded = load_graph(&conn, "test").unwrap().unwrap();

        assert_eq!(loaded.nodes.len(), 2);
        assert_eq!(loaded.edges.len(), 1);
        assert_eq!(loaded.metadata.query, "test_graph");
    }

    #[test]
    fn test_load_nonexistent() {
        let conn = Connection::open_in_memory().unwrap();
        let result = load_graph(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_graph() {
        let conn = Connection::open_in_memory().unwrap();
        let graph = make_graph();
        save_graph(&conn, "test", &graph).unwrap();
        assert!(delete_graph(&conn, "test").unwrap());
        assert!(!delete_graph(&conn, "nonexistent").unwrap());
    }

    #[test]
    fn test_list_graphs() {
        let conn = Connection::open_in_memory().unwrap();
        let graph = make_graph();
        save_graph(&conn, "graph_a", &graph).unwrap();
        save_graph(&conn, "graph_b", &graph).unwrap();

        let names = list_graphs(&conn).unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"graph_a".to_string()));
        assert!(names.contains(&"graph_b".to_string()));
    }

    #[test]
    fn test_overwrite_graph() {
        let conn = Connection::open_in_memory().unwrap();
        let graph1 = make_graph();
        save_graph(&conn, "test", &graph1).unwrap();

        let mut graph2 = EvidenceGraph::empty("overwritten");
        let n = Node::entity(lingshu_memory_episode::EntityRef::new("project", "项目B"));
        graph2.add_node(n);
        save_graph(&conn, "test", &graph2).unwrap();

        let loaded = load_graph(&conn, "test").unwrap().unwrap();
        assert_eq!(loaded.metadata.query, "overwritten");
        assert_eq!(loaded.nodes.len(), 1);
    }
}
