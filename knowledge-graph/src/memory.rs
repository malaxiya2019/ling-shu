//! 图谱记忆 — 将知识图谱用作 Agent 的图结构记忆.
//!
//! 提供结构化记忆接口：不仅存储文本，还存储实体关系和知识拓扑。

use crate::types::*;
use async_trait::async_trait;
use lingshu_core::LsResult;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 图记忆存储 trait.
#[async_trait]
pub trait GraphMemoryStore: Send + Sync {
    /// 存储图谱.
    async fn store_graph(&self, graph: KnowledgeGraph) -> LsResult<()>;

    /// 获取图谱.
    async fn get_graph(&self, name: &str) -> LsResult<Option<KnowledgeGraph>>;

    /// 添加节点.
    async fn add_node(&self, graph_name: &str, node: GraphNode) -> LsResult<bool>;

    /// 添加边.
    async fn add_edge(&self, graph_name: &str, edge: GraphEdge) -> LsResult<bool>;

    /// 搜索节点.
    async fn search_nodes(&self, graph_name: &str, query: &str) -> LsResult<Vec<GraphNode>>;

    /// 获取节点邻居.
    async fn get_neighbors(&self, graph_name: &str, node_id: &str) -> LsResult<Vec<GraphNode>>;

    /// 列出所有图谱名称.
    async fn list_graphs(&self) -> LsResult<Vec<String>>;
}

/// 内存图记忆实现.
#[derive(Clone)]
pub struct GraphMemory {
    graphs: Arc<RwLock<HashMap<String, KnowledgeGraph>>>,
}

impl GraphMemory {
    pub fn new() -> Self {
        Self {
            graphs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 合并 agent 交互记录作为记忆节点.
    pub async fn record_interaction(
        &self,
        graph_name: &str,
        agent_id: &str,
        action: &str,
        summary: &str,
    ) -> LsResult<()> {
        let mut graphs = self.graphs.write().await;
        let graph = graphs.entry(graph_name.to_string()).or_insert_with(|| {
            let mut g = KnowledgeGraph::new(graph_name, "");
            g.kind = GraphKind::Agent;
            g
        });

        let node_id = format!("interaction:{}:{}", agent_id, graph.nodes.len() + 1);
        graph.nodes.push(GraphNode {
            id: node_id.clone(),
            node_type: NodeType::Concept,
            name: action.to_string(),
            file_path: None,
            line_range: None,
            summary: summary.to_string(),
            tags: vec![agent_id.to_string(), "interaction".into()],
            complexity: Complexity::Simple,
            language: None,
            domain_meta: None,
            knowledge_meta: None,
        });

        // 链接到前一个交互
        if graph.nodes.len() > 1 {
            let prev_id = &graph.nodes[graph.nodes.len() - 2].id;
            graph.edges.push(GraphEdge {
                source: prev_id.clone(),
                target: node_id,
                edge_type: EdgeType::Related,
                direction: EdgeDirection::Forward,
                description: Some("chronological".into()),
                weight: 0.5,
            });
        }

        Ok(())
    }

    /// 语义路径查找（BFS 最短路径）.
    pub async fn find_path(
        &self,
        graph_name: &str,
        from_id: &str,
        to_id: &str,
    ) -> LsResult<Vec<String>> {
        let graphs = self.graphs.read().await;
        let graph = graphs.get(graph_name).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("graph '{graph_name}'"))
        })?;

        // BFS
        let mut visited: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<(&str, Vec<String>)> =
            std::collections::VecDeque::new();
        queue.push_back((from_id, vec![from_id.to_string()]));
        visited.insert(from_id);

        while let Some((current, path)) = queue.pop_front() {
            if current == to_id {
                return Ok(path);
            }
            for edge in &graph.edges {
                let neighbor = if edge.source == current { Some(&edge.target) }
                    else if edge.target == current { Some(&edge.source) }
                    else { None };
                if let Some(nid) = neighbor {
                    if visited.insert(nid) {
                        let mut new_path = path.clone();
                        new_path.push(nid.to_string());
                        queue.push_back((nid, new_path));
                    }
                }
            }
        }

        Err(lingshu_core::LsError::NotFound(format!("no path from {from_id} to {to_id}")))
    }
}

impl Default for GraphMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GraphMemoryStore for GraphMemory {
    async fn store_graph(&self, graph: KnowledgeGraph) -> LsResult<()> {
        let name = graph.project.name.clone();
        self.graphs.write().await.insert(name, graph);
        Ok(())
    }

    async fn get_graph(&self, name: &str) -> LsResult<Option<KnowledgeGraph>> {
        Ok(self.graphs.read().await.get(name).cloned())
    }

    async fn add_node(&self, graph_name: &str, node: GraphNode) -> LsResult<bool> {
        let mut graphs = self.graphs.write().await;
        let graph = graphs.get_mut(graph_name).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("graph '{graph_name}'"))
        })?;

        if graph.nodes.iter().any(|n| n.id == node.id) {
            return Ok(false);
        }
        graph.nodes.push(node);
        Ok(true)
    }

    async fn add_edge(&self, graph_name: &str, edge: GraphEdge) -> LsResult<bool> {
        let mut graphs = self.graphs.write().await;
        let graph = graphs.get_mut(graph_name).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("graph '{graph_name}'"))
        })?;

        let key = format!("{}|{}|{}", edge.edge_type.as_str(), edge.source, edge.target);
        if graph.edges.iter().any(|e| {
            format!("{}|{}|{}", e.edge_type.as_str(), e.source, e.target) == key
        }) {
            return Ok(false);
        }
        graph.edges.push(edge);
        Ok(true)
    }

    async fn search_nodes(&self, graph_name: &str, query: &str) -> LsResult<Vec<GraphNode>> {
        let graphs = self.graphs.read().await;
        let graph = graphs.get(graph_name).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("graph '{graph_name}'"))
        })?;

        let q = query.to_lowercase();
        Ok(graph.nodes.iter()
            .filter(|n| {
                n.name.to_lowercase().contains(&q)
                    || n.summary.to_lowercase().contains(&q)
                    || n.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .cloned()
            .collect())
    }

    async fn get_neighbors(&self, graph_name: &str, node_id: &str) -> LsResult<Vec<GraphNode>> {
        let graphs = self.graphs.read().await;
        let graph = graphs.get(graph_name).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("graph '{graph_name}'"))
        })?;

        let neighbor_ids: std::collections::HashSet<String> = graph.edges.iter()
            .filter(|e| e.source == node_id || e.target == node_id)
            .map(|e| if e.source == node_id { &e.target } else { &e.source })
            .cloned()
            .collect();

        Ok(graph.nodes.iter()
            .filter(|n| neighbor_ids.contains(&n.id))
            .cloned()
            .collect())
    }

    async fn list_graphs(&self) -> LsResult<Vec<String>> {
        Ok(self.graphs.read().await.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let mem = GraphMemory::new();
        let graph = KnowledgeGraph::new("my-project", "abc123");
        mem.store_graph(graph).await.unwrap();

        let retrieved = mem.get_graph("my-project").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().project.name, "my-project");
    }

    #[tokio::test]
    async fn test_add_node() {
        let mem = GraphMemory::new();
        let graph = KnowledgeGraph::new("test", "abc");
        mem.store_graph(graph).await.unwrap();

        let added = mem.add_node("test", GraphNode {
            id: "file:main.rs".into(),
            node_type: NodeType::File,
            name: "main.rs".into(),
            file_path: Some("main.rs".into()),
            line_range: None,
            summary: "entry".into(),
            tags: vec![],
            complexity: Complexity::Simple,
            language: None,
            domain_meta: None,
            knowledge_meta: None,
        }).await.unwrap();
        assert!(added);

        // duplicate
        let dup = mem.add_node("test", GraphNode {
            id: "file:main.rs".into(),
            node_type: NodeType::File,
            name: "main.rs".into(),
            file_path: None,
            line_range: None,
            summary: "".into(),
            tags: vec![],
            complexity: Complexity::Simple,
            language: None,
            domain_meta: None,
            knowledge_meta: None,
        }).await.unwrap();
        assert!(!dup);
    }

    #[tokio::test]
    async fn test_search() {
        let _ = tracing_subscriber::fmt::try_init();
        let mem = GraphMemory::new();
        let mut builder = crate::GraphBuilder::new("test", "abc");
        builder.add_knowledge_node("a1", NodeType::Article, "Rust Guide", "Guide to Rust", "docs");
        builder.add_knowledge_node("e1", NodeType::Entity, "ownership", "Ownership system", "concept");
        mem.store_graph(builder.build()).await.unwrap();

        let results = mem.search_nodes("test", "rust").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_interaction_recording() {
        let mem = GraphMemory::new();
        let graph = KnowledgeGraph::new("agent-session", "");
        mem.store_graph(graph).await.unwrap();

        mem.record_interaction("agent-session", "agent-1", "search", "Searched for files").await.unwrap();
        mem.record_interaction("agent-session", "agent-1", "analyze", "Analyzed architecture").await.unwrap();

        let g = mem.get_graph("agent-session").await.unwrap().unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
    }

    use crate::GraphBuilder;

    #[tokio::test]
    async fn test_find_path() {
        let mem = GraphMemory::new();
        let mut builder = GraphBuilder::new("net", "");
        builder.add_node(GraphNode {
            id: "a".into(), node_type: NodeType::Concept, name: "A".into(),
            file_path: None, line_range: None, summary: "".into(), tags: vec![],
            complexity: Complexity::Simple, language: None,
            domain_meta: None, knowledge_meta: None,
        });
        builder.add_node(GraphNode {
            id: "b".into(), node_type: NodeType::Concept, name: "B".into(),
            file_path: None, line_range: None, summary: "".into(), tags: vec![],
            complexity: Complexity::Simple, language: None,
            domain_meta: None, knowledge_meta: None,
        });
        builder.add_node(GraphNode {
            id: "c".into(), node_type: NodeType::Concept, name: "C".into(),
            file_path: None, line_range: None, summary: "".into(), tags: vec![],
            complexity: Complexity::Simple, language: None,
            domain_meta: None, knowledge_meta: None,
        });
        builder.add_edge(GraphEdge {
            source: "a".into(), target: "b".into(), edge_type: EdgeType::Related,
            direction: EdgeDirection::Forward, description: None, weight: 1.0,
        });
        builder.add_edge(GraphEdge {
            source: "b".into(), target: "c".into(), edge_type: EdgeType::Related,
            direction: EdgeDirection::Forward, description: None, weight: 1.0,
        });
        mem.store_graph(builder.build()).await.unwrap();

        let path = mem.find_path("net", "a", "c").await.unwrap();
        assert_eq!(path, vec!["a", "b", "c"]);
    }
}
