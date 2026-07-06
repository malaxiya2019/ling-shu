//! 图谱混合记忆 — 将 KnowledgeGraph 记忆与原有向量/缓冲记忆统一.
//!
//! 提供三种记忆的融合：
//! - `ChatBuffer`: 短期对话缓冲
//! - `VectorMemory`: 长期语义向量存储
//! - `GraphMemory`: 图结构关系记忆（实体关系拓扑）

use lingshu_core::LsResult;
use lingshu_knowledge_graph::{GraphMemory, GraphMemoryStore, GraphNode, KnowledgeGraph};
use serde::{Deserialize, Serialize};

use crate::buffer::ChatBuffer;
use crate::types::{MemoryItem, MemoryQuery};
use crate::vector::{InMemoryVectorStore, VectorMemory};

/// 统一记忆条目（包含图结构信息）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridMemoryItem {
    pub memory: MemoryItem,
    pub graph_node: Option<GraphNode>,
    pub relationship: Option<String>,
}

/// 图谱混合记忆查询结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    pub items: Vec<HybridMemoryItem>,
    pub from_buffer: usize,
    pub from_vector: usize,
    pub from_graph: usize,
}

/// 混合记忆系统.
pub struct HybridMemory {
    pub buffer: ChatBuffer,
    pub vector: InMemoryVectorStore,
    pub graph: GraphMemory,
    graph_enhanced: bool,
}

impl HybridMemory {
    pub fn new(session_id: &str, buffer_capacity: usize, graph_enhanced: bool) -> Self {
        Self {
            buffer: ChatBuffer::new(session_id, buffer_capacity),
            vector: InMemoryVectorStore::new(),
            graph: GraphMemory::new(),
            graph_enhanced,
        }
    }

    pub async fn store(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        _metadata: serde_json::Value,
    ) -> LsResult<MemoryItem> {
        let item = MemoryItem::new(session_id, role, content);
        let item = item.with_metadata(
            "stored_at",
            serde_json::json!(chrono::Utc::now().to_rfc3339()),
        );

        self.buffer.add(item.clone()).await;
        self.vector.store(item.clone(), vec![]).await?;

        if self.graph_enhanced {
            self.graph
                .record_interaction(
                    &format!("session:{session_id}"),
                    role,
                    if role == "user" {
                        "user_message"
                    } else {
                        "assistant_response"
                    },
                    content,
                )
                .await?;
        }

        Ok(item)
    }

    pub async fn search(&self, query: &MemoryQuery) -> LsResult<HybridSearchResult> {
        // 1. 缓冲检索
        let from_buffer: Vec<HybridMemoryItem> = self
            .buffer
            .all()
            .await
            .into_iter()
            .map(|item| HybridMemoryItem {
                memory: item,
                graph_node: None,
                relationship: None,
            })
            .collect();
        let n_buffer = from_buffer.len();

        // 2. 向量检索
        let from_vector: Vec<HybridMemoryItem> = if query.query.is_some() {
            self.vector
                .search_by_text(query, query.limit)
                .await?
                .into_iter()
                .map(|r| HybridMemoryItem {
                    memory: r.item,
                    graph_node: None,
                    relationship: None,
                })
                .collect()
        } else {
            Vec::new()
        };
        let n_vector = from_vector.len();

        // 3. 图记忆检索（仅当有搜索内容时）
        let from_graph: Vec<HybridMemoryItem> = if self.graph_enhanced && query.query.is_some() {
            if let Some(ref session_id) = query.session_id {
                let graph_name = format!("session:{session_id}");
                let search_term = query.query.as_deref().unwrap_or("");
                if let Ok(nodes) = self.graph.search_nodes(&graph_name, search_term).await {
                    nodes
                        .into_iter()
                        .map(|node| HybridMemoryItem {
                            memory: MemoryItem::new(session_id, "system", &node.summary),
                            graph_node: Some(node),
                            relationship: Some("graph_memory".into()),
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let n_graph = from_graph.len();

        // 合并结果（缓冲优先，去重）
        let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut merged = Vec::new();
        for item in from_buffer.into_iter().chain(from_vector).chain(from_graph) {
            if seen_ids.insert(item.memory.id.clone()) {
                merged.push(item);
            }
            if merged.len() >= query.limit {
                break;
            }
        }

        Ok(HybridSearchResult {
            items: merged,
            from_buffer: n_buffer,
            from_vector: n_vector,
            from_graph: n_graph,
        })
    }

    pub async fn find_path(
        &self,
        session_id: &str,
        from_concept: &str,
        to_concept: &str,
    ) -> LsResult<Vec<String>> {
        let graph_name = format!("session:{session_id}");
        self.graph
            .find_path(&graph_name, from_concept, to_concept)
            .await
    }

    pub async fn import_knowledge(
        &self,
        session_id: &str,
        mut graph: KnowledgeGraph,
    ) -> LsResult<()> {
        graph.project.name = format!("session:{session_id}");
        self.graph.store_graph(graph).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_knowledge_graph::{GraphBuilder, NodeType};

    #[tokio::test]
    async fn test_store_and_search() {
        let hm = HybridMemory::new("session-1", 100, true);
        hm.store("session-1", "user", "Hello world", serde_json::json!({}))
            .await
            .unwrap();
        hm.store("session-1", "assistant", "Hi there!", serde_json::json!({}))
            .await
            .unwrap();

        let result = hm
            .search(&MemoryQuery {
                session_id: Some("session-1".into()),
                query: None,
                limit: 10,
                offset: 0,
                min_relevance: None,
            })
            .await
            .unwrap();

        assert_eq!(result.items.len(), 2);
        assert_eq!(result.from_buffer, 2);
    }

    #[tokio::test]
    async fn test_import_and_search_graph() {
        let hm = HybridMemory::new("session-1", 100, true);

        let mut builder = GraphBuilder::new("knowledge-base", "");
        builder.add_knowledge_node(
            "topic:rust",
            NodeType::Topic,
            "Rust",
            "Systems programming language",
            "programming",
        );
        let graph = builder.build();

        hm.import_knowledge("session-1", graph).await.unwrap();

        let result = hm
            .search(&MemoryQuery {
                session_id: Some("session-1".into()),
                query: Some("rust".into()),
                limit: 10,
                offset: 0,
                min_relevance: None,
            })
            .await
            .unwrap();

        assert!(result.from_graph > 0 || !result.items.is_empty());
    }
}
