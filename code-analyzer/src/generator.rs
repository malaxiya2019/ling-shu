//! 图谱生成器 — 将扫描+分析结果转换为 KnowledgeGraph.

use lingshu_knowledge_graph::{GraphBuilder, GraphNode, KnowledgeGraph, NodeType};

use crate::analyzer::{AnalysisResult, ProjectSummary};
use crate::extractor::StructureExtractor;
use crate::scanner::{FileCategory, FileEntry};

/// 图谱生成器.
pub struct GraphGenerator {
    extractor: StructureExtractor,
}

impl Default for GraphGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphGenerator {
    pub fn new() -> Self {
        Self {
            extractor: StructureExtractor::new(),
        }
    }

    /// 从扫描的文件列表中生成知识图谱.
    ///
    /// `project_name`: 项目名称
    /// `git_hash`: git commit hash（可用 "unknown"）
    /// `entries`: 文件扫描结果
    /// `analyses`: 每个文件的语义分析结果
    /// `summary`: 项目概况
    pub fn generate(
        &self,
        project_name: &str,
        git_hash: &str,
        entries: &[FileEntry],
        analyses: &[AnalysisResult],
        summary: &ProjectSummary,
    ) -> KnowledgeGraph {
        let mut builder = GraphBuilder::new(project_name, git_hash);

        // 设置项目元数据
        for (entry, analysis) in entries.iter().zip(analyses.iter()) {
            let content = std::fs::read_to_string(&entry.path).unwrap_or_default();
            let extracted = self.extractor.extract(&content, &entry.language);

            let complexity = analysis.complexity.clone();

            // 为代码文件添加详细分析
            if entry.category == FileCategory::Code {
                let functions: Vec<(&str, &str, [u32; 2])> = extracted
                    .functions
                    .iter()
                    .map(|f| {
                        (
                            f.name.as_str(),
                            analysis
                                .summaries
                                .get(&f.name)
                                .map(|s| s.as_str())
                                .unwrap_or(""),
                            [f.line_start, f.line_end],
                        )
                    })
                    .collect();
                let classes: Vec<(&str, &str, [u32; 2])> = extracted
                    .classes
                    .iter()
                    .map(|c| (c.name.as_str(), "", [c.line_start, c.line_end]))
                    .collect();

                builder.add_file_with_children(
                    &entry.path,
                    &analysis.file_summary,
                    analysis.tags.clone(),
                    complexity,
                    &entry.language,
                    functions,
                    classes,
                );
            } else {
                // 非代码文件
                let node_type = match entry.category {
                    FileCategory::Config => NodeType::Config,
                    FileCategory::Docs => NodeType::Document,
                    FileCategory::Infra => NodeType::Resource,
                    FileCategory::Data => NodeType::Table,
                    FileCategory::Script => NodeType::Step,
                    FileCategory::Markup => NodeType::Document,
                    _ => NodeType::File,
                };

                builder.add_node(GraphNode {
                    id: format!(
                        "{}:{}{}",
                        node_type.as_str(),
                        if entry.path.starts_with("/")
                            || entry.path.starts_with("./")
                            || entry.path.starts_with("..")
                        {
                            ""
                        } else {
                            ""
                        },
                        entry.path
                    ),
                    node_type,
                    name: entry.name.clone(),
                    file_path: Some(entry.path.clone()),
                    line_range: None,
                    summary: analysis.summary.clone(),
                    tags: analysis.tags.clone(),
                    complexity,
                    language: Some(entry.language.clone()),
                    domain_meta: None,
                    knowledge_meta: None,
                });
            }

            // 添加导入边
            for imp in &extracted.imports {
                let target_path = imp.source.replace('.', "/");
                // 尝试解析目标文件
                for other_entry in entries {
                    if other_entry.path.contains(&target_path) {
                        builder.add_import(&entry.path, &other_entry.path);
                        break;
                    }
                }
            }
        }

        let mut graph = builder.build();
        graph.project.description = summary.description.clone();
        graph.project.frameworks = summary.frameworks.clone();

        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{FileCategory, FileEntry};

    fn dummy_entry(path: &str, lang: &str, cat: FileCategory) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            name: path.split('/').last().unwrap_or(path).to_string(),
            extension: path.rsplit('.').next().unwrap_or("").to_string(),
            language: lang.to_string(),
            size_lines: 10,
            category: cat,
        }
    }

    fn dummy_analysis(path: &str) -> AnalysisResult {
        AnalysisResult {
            file_path: path.to_string(),
            summary: format!("File: {path}"),
            tags: vec![],
            complexity: lingshu_knowledge_graph::Complexity::Simple,
            language_notes: None,
            summaries: std::collections::HashMap::new(),
            file_summary: format!("Summary of {path}"),
        }
    }

    #[test]
    fn test_generate_simple() {
        let generator = GraphGenerator::new();
        let entries = vec![
            dummy_entry("src/main.rs", "rust", FileCategory::Code),
            dummy_entry("Cargo.toml", "toml", FileCategory::Config),
            dummy_entry("README.md", "markdown", FileCategory::Docs),
        ];
        let analyses = vec![
            dummy_analysis("src/main.rs"),
            dummy_analysis("Cargo.toml"),
            dummy_analysis("README.md"),
        ];
        let summary = ProjectSummary {
            name: "test-project".into(),
            description: "A test".into(),
            languages: vec!["rust".into()],
            frameworks: vec![],
        };

        let graph = generator.generate("test-project", "abc123", &entries, &analyses, &summary);
        assert_eq!(graph.project.name, "test-project");
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.project.description, "A test");
    }

    #[test]
    fn test_generate_with_children() {
        let generator = GraphGenerator::new();
        let _entries = vec![dummy_entry("src/lib.rs", "rust", FileCategory::Code)];
        // Create a temp file with Rust code so extractor finds functions
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(&file_path, "fn hello() {}\npub fn world() {}").unwrap();

        let mut entry = dummy_entry(file_path.to_str().unwrap(), "rust", FileCategory::Code);
        entry.path = file_path.to_str().unwrap().to_string();
        let mut analysis = dummy_analysis(file_path.to_str().unwrap());
        analysis
            .summaries
            .insert("hello".to_string(), "Greeting function".into());
        analysis
            .summaries
            .insert("world".to_string(), "World function".into());

        let summary = ProjectSummary {
            name: "test".into(),
            description: "".into(),
            languages: vec!["rust".into()],
            frameworks: vec![],
        };

        let graph = generator.generate("test", "abc", &[entry], &[analysis], &summary);
        // Should have file node + 2 function nodes
        assert_eq!(graph.nodes.len(), 3);

        // Should have 2 contains edges
        let contains_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == lingshu_knowledge_graph::EdgeType::Contains)
            .collect();
        assert_eq!(contains_edges.len(), 2);
    }
}
