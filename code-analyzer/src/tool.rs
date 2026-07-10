//! CodeAnalysisTool — 将代码分析流水线暴露为 Tool trait 实现.
//!
//! 可通过 MCP / ToolRegistry 注册，支持从 LLM 调用代码分析能力。

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};
use serde_json::Value;
use std::sync::Arc;

use crate::{llm_analyzer::LlmAnalyzer, FileScanner, GraphGenerator};

/// 代码分析工具.
///
/// 封装代码分析流水线，暴露为 Tool trait 以供 MCP 协议调用。
pub struct CodeAnalysisTool {
    scanner: FileScanner,
    llm_analyzer: Option<Arc<LlmAnalyzer>>,
}

impl CodeAnalysisTool {
    /// 创建新的代码分析工具（规则分析模式）.
    pub fn new() -> Self {
        Self {
            scanner: FileScanner::new(),
            llm_analyzer: None,
        }
    }

    /// 设置 LLM 分析器（启用语义分析）.
    pub fn with_llm(mut self, analyzer: Arc<LlmAnalyzer>) -> Self {
        self.llm_analyzer = Some(analyzer);
        self
    }
}

impl Default for CodeAnalysisTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CodeAnalysisTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "analyze_code".into(),
            description: "Analyze a codebase directory — scan files, extract structure, generate knowledge graph. Optionally enrich with LLM semantics.".into(),
            parameters: vec![
                ToolParam {
                    name: "project_root".into(),
                    description: "Absolute path to the project directory".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "project_name".into(),
                    description: "Name of the project for graph identification".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "enable_semantic".into(),
                    description: "Whether to run LLM semantic enrichment (requires LLM analyzer)".into(),
                    required: false,
                    param_type: "boolean".into(),
                },
                ToolParam {
                    name: "max_files".into(),
                    description: "Maximum number of files to analyze (default 500)".into(),
                    required: false,
                    param_type: "number".into(),
                },
            ],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if input.get("project_root").and_then(|v| v.as_str()).is_none() {
            return Err(lingshu_core::LsError::Validation(
                "missing required field: project_root".into(),
            ));
        }
        if input.get("project_name").and_then(|v| v.as_str()).is_none() {
            return Err(lingshu_core::LsError::Validation(
                "missing required field: project_name".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let project_root = input["project_root"].as_str().unwrap();
        let project_name = input["project_name"].as_str().unwrap();
        let enable_semantic = input
            .get("enable_semantic")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_files = input
            .get("max_files")
            .and_then(|v| v.as_u64())
            .unwrap_or(500) as usize;

        // 1. Scan directory
        let project_path = std::path::Path::new(project_root);
        let entries = self.scanner.scan(project_path)?;

        let file_entries: Vec<_> = entries.iter().take(max_files).cloned().collect();

        // 2. Build default analysis results
        let analyses: Vec<_> = file_entries
            .iter()
            .map(|e| crate::AnalysisResult {
                file_path: e.path.clone(),
                summary: format!("{} file: {}", e.language, e.name),
                tags: vec![e.language.clone(), format!("{:?}", e.category)],
                complexity: lingshu_knowledge_graph::Complexity::Simple,
                language_notes: Some(e.language.clone()),
                summaries: std::collections::HashMap::new(),
                file_summary: format!("{} source file: {}", e.language, e.name),
            })
            .collect();

        // 3. Build project summary
        let languages: Vec<_> = file_entries
            .iter()
            .map(|e| e.language.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let summary = crate::ProjectSummary {
            name: project_name.to_string(),
            description: format!("Code analysis of {}", project_root),
            languages,
            frameworks: vec![],
        };

        // 4. Build knowledge graph
        let generator = GraphGenerator::new();
        let graph = generator.generate(project_name, "unknown", &file_entries, &analyses, &summary);

        // 5. Optional LLM enrichment
        let enrichment_count = if enable_semantic {
            if let Some(ref analyzer) = self.llm_analyzer {
                let queue = crate::EnrichmentQueue::start(analyzer.clone(), analyzer.batch_size());
                let count = file_entries.len();
                for entry in &file_entries {
                    let content = std::fs::read_to_string(&entry.path).unwrap_or_default();
                    queue
                        .submit(crate::EnrichmentTask {
                            file_path: entry.path.clone(),
                            content,
                            language: entry.language.clone(),
                            callback: None,
                        })
                        .await
                        .map_err(|e| {
                            lingshu_core::LsError::Internal(format!(
                                "Failed to submit enrichment: {e}"
                            ))
                        })?;
                }
                count
            } else {
                0
            }
        } else {
            0
        };

        // 6. Return result
        Ok(serde_json::json!({
            "project": project_name,
            "files_scanned": file_entries.len(),
            "graph_nodes": graph.nodes.len(),
            "graph_edges": graph.edges.len(),
            "enrichment_submitted": enrichment_count,
            "languages": summary.languages,
            "summary": {
                "description": summary.description,
                "languages": summary.languages,
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_traits::llm::{Llm, LlmMessage, LlmRequest, LlmResponse, LlmRole, LlmUsage};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[allow(dead_code)]
    struct TestLlm {
        count: AtomicU64,
    }

    #[async_trait]
    impl Llm for TestLlm {
        async fn invoke(&self, _ctx: LsContext, _request: LlmRequest) -> LsResult<LlmResponse> {
            self.count.fetch_add(1, Ordering::Relaxed);
            Ok(LlmResponse {
                message: LlmMessage {
                    role: LlmRole::Assistant,
                    content:
                        r#"{"summary":"test","tags":["test"],"complexity":"Simple","functions":{}}"#
                            .into(),
                    content_parts: None,
                    name: None,
                    tool_calls: None,
                },
                finish_reason: "stop".into(),
                usage: LlmUsage {
                    prompt_tokens: 10,
                    completion_tokens: 10,
                    total_tokens: 20,
                },
            })
        }

        async fn invoke_stream(
            &self,
            _ctx: LsContext,
            _request: LlmRequest,
        ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<lingshu_traits::llm::LlmChunk>>>
        {
            unimplemented!()
        }

        async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
            Ok(HashMap::new())
        }
    }

    #[tokio::test]
    async fn test_validate_missing_fields() {
        let tool = CodeAnalysisTool::new();
        let result = tool.validate(&serde_json::json!({}));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_ok() {
        let tool = CodeAnalysisTool::new();
        let result = tool.validate(&serde_json::json!({
            "project_root": "/tmp",
            "project_name": "test",
        }));
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tool_info() {
        let tool = CodeAnalysisTool::new();
        let info = tool.info();
        assert_eq!(info.name, "analyze_code");
        assert!(info.parameters.iter().any(|p| p.name == "project_root"));
        assert!(info.parameters.iter().any(|p| p.name == "project_name"));
    }
}
