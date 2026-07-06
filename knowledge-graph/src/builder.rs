//! 图构建器 — 参照 UA 的 GraphBuilder，防重复节点/边.

use crate::types::*;
use std::collections::HashSet;

/// 图构建器（去重保证）.
pub struct GraphBuilder {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    node_ids: HashSet<String>,
    edge_keys: HashSet<String>,
    project_name: String,
    git_hash: String,
}

impl GraphBuilder {
    pub fn new(project_name: &str, git_hash: &str) -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            node_ids: HashSet::new(),
            edge_keys: HashSet::new(),
            project_name: project_name.to_string(),
            git_hash: git_hash.to_string(),
        }
    }

    /// 添加节点（去重）.
    pub fn add_node(&mut self, node: GraphNode) -> bool {
        if self.node_ids.contains(&node.id) {
            return false;
        }
        self.node_ids.insert(node.id.clone());
        self.nodes.push(node);
        true
    }

    /// 添加边（去重）.
    pub fn add_edge(&mut self, edge: GraphEdge) -> bool {
        let key = format!(
            "{}|{}|{}",
            edge.edge_type.as_str(),
            edge.source,
            edge.target
        );
        if self.edge_keys.contains(&key) {
            return false;
        }
        self.edge_keys.insert(key);
        self.edges.push(edge);
        true
    }

    /// 添加文件节点 + 包含的函数/类节点（含 contains 边）.
    pub fn add_file_with_children(
        &mut self,
        file_path: &str,
        summary: &str,
        tags: Vec<String>,
        complexity: Complexity,
        language: &str,
        functions: Vec<(&str, &str, [u32; 2])>,
        classes: Vec<(&str, &str, [u32; 2])>,
    ) {
        let file_id = format!("file:{file_path}");
        self.add_node(GraphNode {
            id: file_id.clone(),
            node_type: NodeType::File,
            name: file_path.split('/').next_back().unwrap_or(file_path).to_string(),
            file_path: Some(file_path.to_string()),
            line_range: None,
            summary: summary.to_string(),
            tags,
            complexity,
            language: Some(language.to_string()),
            domain_meta: None,
            knowledge_meta: None,
        });

        for (name, func_summary, range) in functions {
            let func_id = format!("function:{file_path}:{name}");
            self.add_node(GraphNode {
                id: func_id.clone(),
                node_type: NodeType::Function,
                name: name.to_string(),
                file_path: Some(file_path.to_string()),
                line_range: Some(range),
                summary: func_summary.to_string(),
                tags: vec![],
                complexity: Complexity::Simple,
                language: Some(language.to_string()),
                domain_meta: None,
                knowledge_meta: None,
            });
            self.add_edge(GraphEdge {
                source: file_id.clone(),
                target: func_id,
                edge_type: EdgeType::Contains,
                direction: EdgeDirection::Forward,
                description: None,
                weight: 1.0,
            });
        }

        for (name, cls_summary, range) in classes {
            let cls_id = format!("class:{file_path}:{name}");
            self.add_node(GraphNode {
                id: cls_id.clone(),
                node_type: NodeType::Class,
                name: name.to_string(),
                file_path: Some(file_path.to_string()),
                line_range: Some(range),
                summary: cls_summary.to_string(),
                tags: vec![],
                complexity: Complexity::Simple,
                language: Some(language.to_string()),
                domain_meta: None,
                knowledge_meta: None,
            });
            self.add_edge(GraphEdge {
                source: file_id.clone(),
                target: cls_id,
                edge_type: EdgeType::Contains,
                direction: EdgeDirection::Forward,
                description: None,
                weight: 1.0,
            });
        }
    }

    /// 添加导入边.
    pub fn add_import(&mut self, from_file: &str, to_file: &str) {
        self.add_edge(GraphEdge {
            source: format!("file:{from_file}"),
            target: format!("file:{to_file}"),
            edge_type: EdgeType::Imports,
            direction: EdgeDirection::Forward,
            description: None,
            weight: 0.7,
        });
    }

    /// 添加调用边.
    pub fn add_call(&mut self, from_file: &str, from_func: &str, to_file: &str, to_func: &str) {
        self.add_edge(GraphEdge {
            source: format!("function:{from_file}:{from_func}"),
            target: format!("function:{to_file}:{to_func}"),
            edge_type: EdgeType::Calls,
            direction: EdgeDirection::Forward,
            description: None,
            weight: 0.8,
        });
    }

    /// 添加知识节点.
    pub fn add_knowledge_node(
        &mut self,
        id: &str,
        node_type: NodeType,
        name: &str,
        summary: &str,
        category: &str,
    ) {
        self.add_node(GraphNode {
            id: id.to_string(),
            node_type,
            name: name.to_string(),
            file_path: None,
            line_range: None,
            summary: summary.to_string(),
            tags: vec![category.to_string()],
            complexity: Complexity::Simple,
            language: None,
            domain_meta: None,
            knowledge_meta: Some(KnowledgeMeta {
                category: Some(category.to_string()),
                ..Default::default()
            }),
        });
    }

    /// 构建最终图谱.
    pub fn build(self) -> KnowledgeGraph {
        // 收集语言集合
        let mut languages: Vec<String> = self
            .nodes
            .iter()
            .filter_map(|n| n.language.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        languages.sort();

        let mut graph = KnowledgeGraph::new(&self.project_name, &self.git_hash);
        graph.project.languages = languages;
        graph.nodes = self.nodes;
        graph.edges = self.edges;
        graph
    }
}

/// 查询辅助.
impl KnowledgeGraph {
    /// 按类型查询节点.
    pub fn nodes_by_type(&self, node_type: &NodeType) -> Vec<&GraphNode> {
        self.nodes
            .iter()
            .filter(|n| n.node_type == *node_type)
            .collect()
    }

    /// 按标签查询节点.
    pub fn nodes_by_tag(&self, tag: &str) -> Vec<&GraphNode> {
        self.nodes
            .iter()
            .filter(|n| n.tags.contains(&tag.to_string()))
            .collect()
    }

    /// 查询某个节点的出边.
    pub fn outgoing_edges(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.edges.iter().filter(|e| e.source == node_id).collect()
    }

    /// 查询某个节点的入边.
    pub fn incoming_edges(&self, node_id: &str) -> Vec<&GraphEdge> {
        self.edges.iter().filter(|e| e.target == node_id).collect()
    }

    /// 全文搜索节点.
    pub fn search(&self, query: &str) -> Vec<&GraphNode> {
        let q = query.to_lowercase();
        self.nodes
            .iter()
            .filter(|n| {
                n.name.to_lowercase().contains(&q)
                    || n.summary.to_lowercase().contains(&q)
                    || n.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// 获取文件导入图.
    pub fn file_import_graph(&self) -> Vec<(&GraphNode, &GraphNode)> {
        let file_nodes: HashSet<String> = self
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::File)
            .map(|n| n.id.clone())
            .collect();

        self.edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Imports)
            .filter_map(|e| {
                let src = self.nodes.iter().find(|n| n.id == e.source)?;
                let tgt = self.nodes.iter().find(|n| n.id == e.target)?;
                if file_nodes.contains(&src.id) && file_nodes.contains(&tgt.id) {
                    Some((src, tgt))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_empty() {
        let builder = GraphBuilder::new("test", "abc123");
        let graph = builder.build();
        assert_eq!(graph.nodes.len(), 0);
        assert_eq!(graph.edges.len(), 0);
        assert_eq!(graph.project.name, "test");
    }

    #[test]
    fn test_builder_dedup_nodes() {
        let mut builder = GraphBuilder::new("test", "abc");
        let n1 = GraphNode {
            id: "file:src/main.rs".into(),
            node_type: NodeType::File,
            name: "main.rs".into(),
            file_path: Some("src/main.rs".into()),
            line_range: None,
            summary: "Entry point".into(),
            tags: vec![],
            complexity: Complexity::Simple,
            language: Some("rust".into()),
            domain_meta: None,
            knowledge_meta: None,
        };
        assert!(builder.add_node(n1.clone()));
        assert!(!builder.add_node(n1)); // duplicate
    }

    #[test]
    fn test_builder_dedup_edges() {
        let mut builder = GraphBuilder::new("test", "abc");
        builder.add_node(GraphNode {
            id: "file:a.rs".into(),
            node_type: NodeType::File,
            name: "a.rs".into(),
            file_path: Some("a.rs".into()),
            line_range: None,
            summary: "".into(),
            tags: vec![],
            complexity: Complexity::Simple,
            language: None,
            domain_meta: None,
            knowledge_meta: None,
        });
        builder.add_node(GraphNode {
            id: "file:b.rs".into(),
            node_type: NodeType::File,
            name: "b.rs".into(),
            file_path: Some("b.rs".into()),
            line_range: None,
            summary: "".into(),
            tags: vec![],
            complexity: Complexity::Simple,
            language: None,
            domain_meta: None,
            knowledge_meta: None,
        });

        assert!(builder.add_edge(GraphEdge {
            source: "file:a.rs".into(),
            target: "file:b.rs".into(),
            edge_type: EdgeType::Imports,
            direction: EdgeDirection::Forward,
            description: None,
            weight: 0.7,
        }));
        assert!(!builder.add_edge(GraphEdge {
            source: "file:a.rs".into(),
            target: "file:b.rs".into(),
            edge_type: EdgeType::Imports,
            direction: EdgeDirection::Forward,
            description: None,
            weight: 0.7,
        }));
    }

    #[test]
    fn test_add_file_with_children() {
        let mut builder = GraphBuilder::new("test", "abc");
        builder.add_file_with_children(
            "src/lib.rs",
            "Library root",
            vec!["rust".into()],
            Complexity::Moderate,
            "rust",
            vec![("run", "Main function", [1u32, 50u32])],
            vec![("Config", "App config", [60u32, 120u32])],
        );
        let graph = builder.build();
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert!(graph
            .edges
            .iter()
            .all(|e| e.edge_type == EdgeType::Contains));
    }

    #[test]
    fn test_search() {
        let mut builder = GraphBuilder::new("test", "abc");
        builder.add_knowledge_node(
            "article:1",
            NodeType::Article,
            "Rust Book",
            "The Rust Programming Language",
            "documentation",
        );
        builder.add_knowledge_node(
            "entity:1",
            NodeType::Entity,
            "ownership",
            "Rust ownership system",
            "concept",
        );

        let graph = builder.build();
        let results = graph.search("rust");
        assert_eq!(results.len(), 2);
        let results = graph.search("ownership");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_nodes_by_type() {
        let mut builder = GraphBuilder::new("test", "abc");
        builder.add_knowledge_node("a1", NodeType::Article, "A", "Article A", "docs");
        builder.add_knowledge_node("a2", NodeType::Article, "B", "Article B", "docs");
        builder.add_file_with_children(
            "f1.rs",
            "file",
            vec![],
            Complexity::Simple,
            "rust",
            vec![],
            vec![],
        );

        let graph = builder.build();
        assert_eq!(graph.nodes_by_type(&NodeType::Article).len(), 2);
        assert_eq!(graph.nodes_by_type(&NodeType::File).len(), 1);
    }
}
