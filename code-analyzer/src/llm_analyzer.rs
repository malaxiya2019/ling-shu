//! LLM 语义分析器 — 对接 OpenAI 兼容 API 对代码进行语义分析.
//!
//! 设计原则:
//! - 通过 `Arc<dyn Llm>` 注入，不依赖具体 LLM 提供商
//! - 规则分析做快速首轮，LLM 做异步 enrichment
//! - Prompt 模板可配置，支持自定义

use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use lingshu_knowledge_graph::Complexity;
use lingshu_traits::llm::{Llm, LlmMessage, LlmRequest, LlmRole};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::analyzer::{AnalysisResult, ProjectSummary, SemanticAnalyzer};

/// LLM 分析配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAnalyzerConfig {
    /// LLM 模型名称（默认 gpt-4o-mini）.
    pub model: String,
    /// 温度参数（默认 0.1 — 低温度保证一致性）.
    pub temperature: f64,
    /// 摘要最大 token 数.
    pub max_summary_tokens: u32,
    /// 是否启用批量处理.
    pub batch_enabled: bool,
    /// 批次大小（同时分析文件数）.
    pub batch_size: usize,
}

impl Default for LlmAnalyzerConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o-mini".into(),
            temperature: 0.1,
            max_summary_tokens: 256,
            batch_enabled: true,
            batch_size: 5,
        }
    }
}

/// LLM 驱动语义分析器.
pub struct LlmAnalyzer {
    llm: Arc<dyn Llm>,
    config: LlmAnalyzerConfig,
}

impl LlmAnalyzer {
    /// 创建 LLM 分析器.
    pub fn new(llm: Arc<dyn Llm>, config: LlmAnalyzerConfig) -> Self {
        Self { llm, config }
        }

    /// 获取 batch_size 配置.
    pub fn batch_size(&self) -> usize {
        self.config.batch_size
    }

    /// 获取模型名称.
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// 获取配置引用.
    pub fn config(&self) -> &LlmAnalyzerConfig {
        &self.config
}

    /// 构建文件分析 prompt.
    fn build_file_prompt(file_path: &str, content: &str, language: &str) -> String {
        // 截断过长内容
        let max_content_len = 8000;
        let truncated = if content.len() > max_content_len {
            format!("{}\n\n... [truncated, {} bytes total]",
                &content[..max_content_len], content.len())
        } else {
            content.to_string()
        };

        format!(
            r#"Analyze this {lang} source file.

File: {path}

```{lang}
{code}
```

Respond in JSON with exactly these fields:
- "summary": one-sentence semantic summary of what this file does
- "tags": array of 3-5 relevant tags (e.g. "authentication", "data-pipeline", "cli")
- "complexity": one of "Simple", "Moderate", "Complex", "VeryComplex"
- "functions": object mapping function names to one-sentence descriptions
- "file_summary": detailed 2-3 sentence description of the file's purpose

Return ONLY valid JSON, no markdown fencing."#,
            lang = language,
            path = file_path,
            code = truncated,
        )
    }

    /// 分析单个文件（调用 LLM）.
    async fn analyze_file_inner(
        &self,
        file_path: &str,
        content: &str,
        language: &str,
    ) -> LsResult<AnalysisResult> {
        let prompt = Self::build_file_prompt(file_path, content, language);

        let request = LlmRequest {
            model: self.config.model.clone(),
            messages: vec![
                LlmMessage {
                    role: LlmRole::System,
                    content: "You are a precise code analysis engine. Always respond in JSON.".into(),
                    content_parts: None,
                    name: None,
                    tool_calls: None,
                },
                LlmMessage {
                    role: LlmRole::User,
                    content: prompt,
                    content_parts: None,
                    name: None,
                    tool_calls: None,
                },
            ],
            temperature: Some(self.config.temperature),
            max_tokens: Some(self.config.max_summary_tokens),
            tools: None,
            stream: false,
        };

        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let response = self.llm.invoke(ctx, request).await?;

        // Parse JSON response
        let text = response.message.content.trim().to_string();
        // Strip markdown code fences if LLM included them
        let text = text
            .strip_prefix("```json")
            .or_else(|| text.strip_prefix("```"))
            .and_then(|s| s.strip_suffix("```"))
            .unwrap_or(&text)
            .trim()
            .to_string();

        #[derive(Deserialize)]
        struct LlmFileResponse {
            summary: String,
            tags: Vec<String>,
            complexity: String,
            #[serde(default)]
            functions: std::collections::HashMap<String, String>,
            #[serde(default)]
            file_summary: Option<String>,
        }

        let parsed: LlmFileResponse = serde_json::from_str(&text)
            .map_err(|e| lingshu_core::LsError::Internal(format!(
                "LLM response parse failed: {e}\nRaw: {text}",
            )))?;

        let complexity = match parsed.complexity.to_lowercase().as_str() {
            c if c.contains("simple") => Complexity::Simple,
            c if c.contains("moderate") => Complexity::Moderate,
            c if c.contains("complex") => Complexity::Complex,
            _ => Complexity::Simple,
        };

        let summary = parsed.summary;
        let file_summary = parsed.file_summary.unwrap_or_else(|| summary.clone());
        Ok(AnalysisResult {
            file_path: file_path.to_string(),
            summary,
            tags: parsed.tags,
            complexity,
            language_notes: Some(language.to_string()),
            summaries: parsed.functions,
            file_summary,
        })
    }
}

#[async_trait]
impl SemanticAnalyzer for LlmAnalyzer {
    async fn analyze_file(
        &self,
        file_path: &str,
        content: &str,
        language: &str,
    ) -> LsResult<AnalysisResult> {
        self.analyze_file_inner(file_path, content, language).await
    }

    async fn analyze_project(&self, project_name: &str) -> LsResult<ProjectSummary> {
        let prompt = format!(
            r#"Given the project name "{name}", describe what kind of project it might be.
Respond in JSON:
- "description": brief description
- "languages": empty array
- "frameworks": empty array

Return ONLY valid JSON."#,
            name = project_name
        );

        let request = LlmRequest {
            model: self.config.model.clone(),
            messages: vec![
                LlmMessage {
                    role: LlmRole::System,
                    content: "You are a project analysis assistant.".into(),
                    content_parts: None,
                    name: None,
                    tool_calls: None,
                },
                LlmMessage {
                    role: LlmRole::User,
                    content: prompt,
                    content_parts: None,
                    name: None,
                    tool_calls: None,
                },
            ],
            temperature: Some(self.config.temperature),
            max_tokens: Some(128),
            tools: None,
            stream: false,
        };

        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let response = self.llm.invoke(ctx, request).await?;

        #[derive(Deserialize)]
        struct LlmProjectResponse {
            description: String,
            #[serde(default)]
            languages: Vec<String>,
            #[serde(default)]
            frameworks: Vec<String>,
        }

        let parsed: LlmProjectResponse = serde_json::from_str(&response.message.content)
            .unwrap_or_else(|_| LlmProjectResponse {
                description: format!("Project: {project_name}"),
                languages: vec![],
                frameworks: vec![],
            });

        Ok(ProjectSummary {
            name: project_name.to_string(),
            description: parsed.description,
            languages: parsed.languages,
            frameworks: parsed.frameworks,
        })
    }
}

// ── Enrichment Queue ────────────────────────────────

/// Enrichment 任务.
#[derive(Clone)]
pub struct EnrichmentTask {
    pub file_path: String,
    pub content: String,
    pub language: String,
    pub callback: Option<std::sync::Arc<dyn EnrichmentCallback + Send + Sync>>,
}

/// Enrichment 完成回调.
pub trait EnrichmentCallback: Send + Sync {
    fn on_result(&self, result: &AnalysisResult);
    fn on_error(&self, file_path: &str, error: &str);
}

/// 异步 Enrichment 队列.
///
/// 规则分析完成后，将文件提交到队列，后台逐个调用 LLM 进行语义 enrichment.
pub struct EnrichmentQueue {
    sender: tokio::sync::mpsc::Sender<EnrichmentTask>,
    /// 已处理计数.
    pub processed: std::sync::atomic::AtomicUsize,
    /// 总任务数.
    pub total: std::sync::atomic::AtomicUsize,
}

impl EnrichmentQueue {
    /// 创建 enrichment 队列并启动后台处理.
    pub fn start(
        analyzer: Arc<LlmAnalyzer>,
        batch_size: usize,
    ) -> Arc<Self> {
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<EnrichmentTask>(1024);
        let queue = Arc::new(Self {
            sender,
            processed: std::sync::atomic::AtomicUsize::new(0),
            total: std::sync::atomic::AtomicUsize::new(0),
        });

        let _queue_clone = queue.clone();
        tokio::spawn(async move {
            let mut batch: Vec<EnrichmentTask> = Vec::with_capacity(batch_size);

            loop {
                // Collect a batch
                batch.clear();
                while batch.len() < batch_size {
                    match receiver.recv().await {
                        Some(task) => batch.push(task),
                        None => {
                            // Channel closed, process remaining and exit
                            Self::process_batch(analyzer.clone(), &mut batch).await;
                            return;
                        }
                    }
                }
                Self::process_batch(analyzer.clone(), &mut batch).await;
            }
        });

        queue
    }

    async fn process_batch(analyzer: Arc<LlmAnalyzer>, batch: &mut Vec<EnrichmentTask>) {
        if batch.is_empty() {
            return;
        }

        let mut handles = Vec::new();
        for task in batch.drain(..) {
            let analyzer_ref = analyzer.clone();
            handles.push(tokio::spawn(async move {
                match analyzer_ref
                    .analyze_file_inner(&task.file_path, &task.content, &task.language)
                    .await
                {
                    Ok(result) => {
                        if let Some(ref cb) = task.callback {
                            cb.on_result(&result);
                        }
                    }
                    Err(e) => {
                        if let Some(ref cb) = task.callback {
                            cb.on_error(&task.file_path, &e.to_string());
                        }
                    }
                }
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }
    }

    /// 提交 enrichment 任务.
    pub async fn submit(&self, task: EnrichmentTask) -> LsResult<()> {
        self.total.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.sender.send(task).await.map_err(|_| {
            lingshu_core::LsError::Internal("enrichment queue closed".into())
        })?;
        Ok(())
    }
}

// ── Tests ───────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_traits::llm::{LlmChunk, LlmResponse, LlmUsage};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Mock LLM for testing.
    struct TestLlm {
        prompt_tokens: AtomicU64,
    }

    impl TestLlm {
        fn new() -> Self {
            Self { prompt_tokens: AtomicU64::new(0) }
        }
    }

    #[async_trait]
    impl Llm for TestLlm {
        async fn invoke(&self, _ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
            self.prompt_tokens.fetch_add(1, Ordering::Relaxed);

            // Return a mock JSON response based on the prompt content
            let response = if request.messages.iter().any(|m| m.content.contains("Analyze this")) {
                r#"{
                    "summary": "Handles user authentication with JWT tokens",
                    "tags": ["authentication", "security", "jwt"],
                    "complexity": "Moderate",
                    "functions": {
                        "login": "Validates credentials and returns JWT",
                        "verify": "Verifies JWT token validity"
                    },
                    "file_summary": "Implements JWT-based authentication including login, token verification, and refresh logic."
                }"#.to_string()
            } else {
                r#"{"description": "A test project", "languages": [], "frameworks": []}"#.to_string()
            };

            Ok(LlmResponse {
                message: lingshu_traits::llm::LlmMessage {
                    role: LlmRole::Assistant,
                    content: response,
                    content_parts: None,
                    name: None,
                    tool_calls: None,
                },
                finish_reason: "stop".into(),
                usage: LlmUsage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    total_tokens: 150,
                },
            })
        }

        async fn invoke_stream(
            &self,
            _ctx: LsContext,
            _request: LlmRequest,
        ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
            unimplemented!("stream not used in tests")
        }

        async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
            let mut map = HashMap::new();
            map.insert("prompt_tokens".into(), self.prompt_tokens.load(Ordering::Relaxed));
            Ok(map)
        }
    }

    #[tokio::test]
    async fn test_llm_analyzer_file() {
        let llm = Arc::new(TestLlm::new());
        let analyzer = LlmAnalyzer::new(llm, LlmAnalyzerConfig::default());

        let result = analyzer
            .analyze_file("src/auth.rs", "fn login() {}\nfn verify() {}", "rust")
            .await
            .unwrap();

        assert_eq!(result.summary, "Handles user authentication with JWT tokens");
        assert!(result.tags.contains(&"authentication".to_string()));
        assert_eq!(result.complexity, Complexity::Moderate);
        assert!(result.summaries.contains_key("login"));
    }

    #[tokio::test]
    async fn test_llm_analyzer_project() {
        let llm = Arc::new(TestLlm::new());
        let analyzer = LlmAnalyzer::new(llm, LlmAnalyzerConfig::default());

        let result = analyzer.analyze_project("my-project").await.unwrap();
        assert_eq!(result.description, "A test project");
    }

    #[tokio::test]
    async fn test_enrichment_queue() {
        let llm = Arc::new(TestLlm::new());
        let analyzer = Arc::new(LlmAnalyzer::new(llm, LlmAnalyzerConfig::default()));

        let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let results_clone = results.clone();

        struct Callback {
            results: std::sync::Arc<std::sync::Mutex<Vec<AnalysisResult>>>,
        }
        impl EnrichmentCallback for Callback {
            fn on_result(&self, result: &AnalysisResult) {
                if let Ok(mut r) = self.results.lock() {
                    r.push(result.clone());
                }
            }
            fn on_error(&self, _file_path: &str, _error: &str) {}
        }

        // Use batch_size=1 so single task gets processed immediately
        let queue = EnrichmentQueue::start(analyzer, 1);

        queue.submit(EnrichmentTask {
            file_path: "src/auth.rs".into(),
            content: "fn login() {}".into(),
            language: "rust".into(),
            callback: Some(std::sync::Arc::new(Callback { results: results_clone })),
        }).await.unwrap();

        // Give queue time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Give queue time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let locked = results.lock().unwrap();
        assert_eq!(locked.len(), 1);
        assert_eq!(locked[0].summary, "Handles user authentication with JWT tokens");
    }
}
