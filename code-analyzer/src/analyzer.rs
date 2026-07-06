//! 语义分析器 — LLM 驱动的代码摘要、标签、复杂度评估.

use async_trait::async_trait;
use lingshu_core::LsResult;
use lingshu_knowledge_graph::Complexity;
use serde::{Deserialize, Serialize};

/// 分析结果（单个文件）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub file_path: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub complexity: Complexity,
    pub language_notes: Option<String>,
    /// 每个函数/类的摘要.
    pub summaries: std::collections::HashMap<String, String>,
    pub file_summary: String,
}

/// 项目概况结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub name: String,
    pub description: String,
    pub languages: Vec<String>,
    pub frameworks: Vec<String>,
}

/// 语义分析器 trait.
///
/// 实现方可以使用 LLM 后端进行语义分析，也可以提供模拟/规则实现用于测试。
#[async_trait]
pub trait SemanticAnalyzer: Send + Sync {
    /// 分析单个文件.
    async fn analyze_file(&self, file_path: &str, content: &str, language: &str)
        -> LsResult<AnalysisResult>;

    /// 分析项目概况.
    async fn analyze_project(&self, project_name: &str) -> LsResult<ProjectSummary>;
}

/// 基于规则的默认语义分析器（无需 LLM，用于测试）.
pub struct DefaultAnalyzer;

#[async_trait]
impl SemanticAnalyzer for DefaultAnalyzer {
    async fn analyze_file(
        &self,
        file_path: &str,
        _content: &str,
        language: &str,
    ) -> LsResult<AnalysisResult> {
        let name = file_path.split('/').last().unwrap_or(file_path);
        let summary = format!("{language} source file: {name}");

        let mut tags = vec![language.to_string()];
        if file_path.contains("test") || file_path.contains("spec") {
            tags.push("test".into());
        }
        if file_path.contains("main") || file_path.contains("index") {
            tags.push("entry-point".into());
        }

        Ok(AnalysisResult {
            file_path: file_path.to_string(),
            summary: summary.clone(),
            tags,
            complexity: Complexity::Simple,
            language_notes: None,
            summaries: std::collections::HashMap::new(),
            file_summary: summary,
        })
    }

    async fn analyze_project(&self, project_name: &str) -> LsResult<ProjectSummary> {
        Ok(ProjectSummary {
            name: project_name.to_string(),
            description: format!("Project: {project_name}"),
            languages: Vec::new(),
            frameworks: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_analyzer() {
        let analyzer = DefaultAnalyzer;
        let result = analyzer.analyze_file("src/main.rs", "fn main() {}", "rust").await.unwrap();
        assert!(result.summary.contains("rust"));
        assert!(result.tags.contains(&"entry-point".to_string()));
    }

    #[tokio::test]
    async fn test_default_analyzer_test_file() {
        let analyzer = DefaultAnalyzer;
        let result = analyzer.analyze_file("tests/test_foo.rs", "fn test() {}", "rust").await.unwrap();
        assert!(result.tags.contains(&"test".to_string()));
    }
}
