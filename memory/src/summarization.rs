//! MemorySummarizer — 记忆摘要与压缩
//!
//! 使用 LLM 对长对话缓冲进行摘要压缩，减少上下文窗口占用。
//! 支持增量摘要（追加式）和完整摘要（重写式）。
//!
//! # 架构
//!
//! ```text
//! ChatBuffer (N items)
//!     │
//!     ├── threshold exceeded? ───→ MemorySummarizer
//!     │                               │
//!     │                         ┌─────┴──────┐
//!     │                         │  LLM 调用   │
//!     │                         └─────┬──────┘
//!     │                               │
//!     │                         ┌─────┴──────┐
//!     │                         │ 摘要结果    │
//!     │                         │ (结构化)    │
//!     │                         └─────┬──────┘
//!     │                               │
//!     ├── store summary ──────────────┘
//!     │
//!     └── truncate buffer to keep only recent N items + summary
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lingshu_core::{LsContext, LsResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

/// 摘要压缩策略
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummarizationStrategy {
    /// 增量式：追加新摘要到已有摘要之上
    #[default]
    Incremental,
    /// 重写式：丢弃旧摘要，基于全部内容重新生成
    Rewrite,
}

/// 记忆摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySummary {
    /// 摘要 ID
    pub id: String,
    /// 会话 ID
    pub session_id: String,
    /// 摘要文本
    pub summary: String,
    /// 摘要涉及的时间范围（起始）
    pub time_start: DateTime<Utc>,
    /// 摘要涉及的时间范围（结束）
    pub time_end: DateTime<Utc>,
    /// 原始条目数量
    pub original_count: usize,
    /// 基础摘要（增量累积用）
    pub base_summary: Option<String>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 策略
    pub strategy: SummarizationStrategy,
}

impl MemorySummary {
    pub fn new(
        session_id: &str,
        summary: String,
        time_start: DateTime<Utc>,
        time_end: DateTime<Utc>,
        original_count: usize,
        base_summary: Option<String>,
        strategy: SummarizationStrategy,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            summary,
            time_start,
            time_end,
            original_count,
            base_summary,
            created_at: Utc::now(),
            strategy,
        }
    }
}

/// 摘要配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizationConfig {
    /// 触发摘要的缓冲条目数阈值
    pub buffer_threshold: usize,
    /// 摘要后保留的最近条目数
    pub keep_recent: usize,
    /// 摘要策略
    pub strategy: SummarizationStrategy,
    /// 摘要 prompt 模板
    pub summary_prompt_template: String,
    /// 增量摘要 prompt 模板
    pub incremental_prompt_template: String,
    /// 最大摘要长度（token 数估计）
    pub max_summary_length: usize,
    /// 摘要 LLM 模型名
    pub model: Option<String>,
}

impl Default for SummarizationConfig {
    fn default() -> Self {
        Self {
            buffer_threshold: 50,
            keep_recent: 20,
            strategy: SummarizationStrategy::Incremental,
            summary_prompt_template: SUMMARY_PROMPT.to_string(),
            incremental_prompt_template: INCREMENTAL_SUMMARY_PROMPT.to_string(),
            max_summary_length: 1024,
            model: None,
        }
    }
}

/// 摘要 LLM 接口
#[async_trait]
pub trait SummarizerLlm: Send + Sync {
    /// 调用 LLM 生成文本
    async fn generate(&self, ctx: &LsContext, prompt: &str, model: &str) -> LsResult<String>;
}

/// 为 Arc<dyn Llm> 实现 SummarizerLlm
#[async_trait]
impl SummarizerLlm for Arc<dyn lingshu_traits::llm::Llm> {
    async fn generate(&self, ctx: &LsContext, prompt: &str, model: &str) -> LsResult<String> {
        use lingshu_traits::llm::{LlmMessage, LlmRequest, LlmRole};

        let request = LlmRequest {
            model: model.to_string(),
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: prompt.to_string(),
                content_parts: None,
                name: None,
                tool_calls: None,
            }],
            temperature: Some(0.3),
            max_tokens: Some(2048),
            tools: None,
            stream: false,
        };

        let response = self.invoke(ctx.clone(), request).await?;
        Ok(response.message.content)
    }
}

// ── Prompt 模板 ─────────────────────────────────────

/// 完整摘要 prompt
const SUMMARY_PROMPT: &str = r#"请对以下对话内容进行简洁的中文摘要，保留所有重要信息、关键决策、用户偏好和上下文。

对话内容：
{conversation}

请以结构化格式输出摘要：
1. **核心话题**：本次对话涉及的主要话题
2. **关键信息**：重要的事实、决策、用户偏好
3. **行动计划**：需要后续执行的任务或承诺
4. **摘要**：200字以内的连贯摘要
"#;

/// 增量摘要 prompt
const INCREMENTAL_SUMMARY_PROMPT: &str = r#"你已有以下历史摘要：
{previous_summary}

现在有新的对话内容需要合并到摘要中：

新对话内容：
{new_conversation}

请以结构化格式输出更新后的完整摘要：
1. **核心话题**：所有涉及的主要话题（包括历史和新的）
2. **关键信息**：所有重要的事实、决策、用户偏好
3. **行动计划**：所有待执行的任务
4. **摘要**：300字以内的连贯摘要（包含历史和新内容）
"#;

// ── 摘要管理器 ──────────────────────────────────────

/// 记忆摘要管理器
pub struct MemorySummarizer {
    config: SummarizationConfig,
    llm: Option<Arc<dyn SummarizerLlm>>,
}

impl MemorySummarizer {
    /// 创建新的摘要管理器
    pub fn new(config: SummarizationConfig) -> Self {
        Self { config, llm: None }
    }

    /// 设置 LLM 后端
    pub fn with_llm(mut self, llm: Arc<dyn SummarizerLlm>) -> Self {
        self.llm = Some(llm);
        self
    }

    /// 检查是否需要触发摘要
    pub fn should_summarize(&self, buffer_len: usize) -> bool {
        self.llm.is_some() && buffer_len >= self.config.buffer_threshold
    }

    /// 是否需要从缓冲中裁剪
    pub fn should_truncate(&self, buffer_len: usize) -> bool {
        buffer_len >= self.config.buffer_threshold + self.config.keep_recent
    }

    /// 生成对话摘要
    pub async fn summarize(
        &self,
        ctx: &LsContext,
        session_id: &str,
        conversation: &[crate::types::MemoryItem],
        previous_summary: Option<&MemorySummary>,
    ) -> LsResult<Option<MemorySummary>> {
        let llm = match &self.llm {
            Some(l) => l,
            None => return Ok(None),
        };

        if conversation.is_empty() {
            return Ok(None);
        }

        let model = self.config.model.as_deref().unwrap_or("gpt-4o-mini");

        let (prompt, strategy) = match previous_summary {
            Some(prev) if self.config.strategy == SummarizationStrategy::Incremental => {
                let conv_text = format_conversation(conversation);
                let prompt = self
                    .config
                    .incremental_prompt_template
                    .replace("{previous_summary}", &prev.summary)
                    .replace("{new_conversation}", &conv_text);
                (prompt, SummarizationStrategy::Incremental)
            }
            _ => {
                let conv_text = format_conversation(conversation);
                let prompt = self
                    .config
                    .summary_prompt_template
                    .replace("{conversation}", &conv_text);
                (prompt, SummarizationStrategy::Rewrite)
            }
        };

        debug!(
            "summarize: session={}, items={}, strategy={:?}",
            session_id,
            conversation.len(),
            strategy
        );

        let result = llm.generate(ctx, &prompt, model).await?;

        let time_start = conversation
            .first()
            .map(|i| i.timestamp)
            .unwrap_or_else(Utc::now);
        let time_end = conversation
            .last()
            .map(|i| i.timestamp)
            .unwrap_or_else(Utc::now);

        let summary = MemorySummary::new(
            session_id,
            result,
            time_start,
            time_end,
            conversation.len(),
            previous_summary.map(|s| s.summary.clone()),
            strategy,
        );

        info!(
            "summarization complete: session={}, items={}, strategy={:?}",
            session_id,
            conversation.len(),
            strategy
        );

        Ok(Some(summary))
    }

    /// 获取配置引用
    pub fn config(&self) -> &SummarizationConfig {
        &self.config
    }
}

impl Default for MemorySummarizer {
    fn default() -> Self {
        Self::new(SummarizationConfig::default())
    }
}

/// 将对话条目格式化为文本
fn format_conversation(items: &[crate::types::MemoryItem]) -> String {
    items
        .iter()
        .map(|item| {
            let ts = item.timestamp.format("%H:%M:%S");
            format!("[{}] {}: {}", ts, item.role, item.content)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── 测试 ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MemoryItem;

    #[test]
    fn test_summarization_config_defaults() {
        let config = SummarizationConfig::default();
        assert_eq!(config.buffer_threshold, 50);
        assert_eq!(config.keep_recent, 20);
        assert_eq!(config.strategy, SummarizationStrategy::Incremental);
    }

    #[test]
    fn test_should_summarize_no_llm() {
        let summarizer = MemorySummarizer::default();
        assert!(!summarizer.should_summarize(50));
        assert!(!summarizer.should_summarize(100));
    }

    #[test]
    fn test_should_summarize_with_llm() {
        struct MockLlm;
        #[async_trait]
        impl SummarizerLlm for MockLlm {
            async fn generate(
                &self,
                _ctx: &LsContext,
                _prompt: &str,
                _model: &str,
            ) -> LsResult<String> {
                Ok("test summary".to_string())
            }
        }

        let summarizer = MemorySummarizer::default().with_llm(Arc::new(MockLlm));

        assert!(!summarizer.should_summarize(10));
        assert!(summarizer.should_summarize(50));
        assert!(summarizer.should_summarize(100));

        // should_truncate 在 buffer_threshold + keep_recent 时触发
        assert!(!summarizer.should_truncate(69));
        assert!(summarizer.should_truncate(70));
    }

    #[test]
    fn test_format_conversation() {
        let items = vec![
            MemoryItem::new("s1", "user", "你好"),
            MemoryItem::new("s1", "assistant", "你好！有什么可以帮助你的？"),
        ];
        let text = format_conversation(&items);
        assert!(text.contains("user: 你好"));
        assert!(text.contains("assistant: 你好！"));
    }

    #[tokio::test]
    async fn test_summarize_empty() {
        let summarizer = MemorySummarizer::default();
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let result = summarizer.summarize(&ctx, "s1", &[], None).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_summarize_no_llm() {
        let summarizer = MemorySummarizer::default();
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let items = vec![MemoryItem::new("s1", "user", "hello")];
        let result = summarizer
            .summarize(&ctx, "s1", &items, None)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_summarize_with_mock_llm() {
        struct MockLlm;
        #[async_trait]
        impl SummarizerLlm for MockLlm {
            async fn generate(
                &self,
                _ctx: &LsContext,
                _prompt: &str,
                _model: &str,
            ) -> LsResult<String> {
                Ok(
                    "**摘要**：这是一个测试摘要。\n**核心话题**：测试\n**关键信息**：模拟 LLM 调用"
                        .to_string(),
                )
            }
        }

        let summarizer = MemorySummarizer::default().with_llm(Arc::new(MockLlm));
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let items = vec![
            MemoryItem::new("s1", "user", "今天天气怎么样？"),
            MemoryItem::new("s1", "assistant", "今天天气很好。"),
        ];

        let result = summarizer
            .summarize(&ctx, "s1", &items, None)
            .await
            .unwrap();
        assert!(result.is_some());
        let summary = result.unwrap();
        assert_eq!(summary.session_id, "s1");
        assert_eq!(summary.original_count, 2);
        assert_eq!(summary.strategy, SummarizationStrategy::Rewrite);
        assert!(summary.summary.contains("测试"));
    }

    #[tokio::test]
    async fn test_incremental_summarize() {
        struct MockLlm;
        #[async_trait]
        impl SummarizerLlm for MockLlm {
            async fn generate(
                &self,
                _ctx: &LsContext,
                _prompt: &str,
                _model: &str,
            ) -> LsResult<String> {
                Ok("**摘要**：增量摘要结果。\n**核心话题**：测试续接".to_string())
            }
        }

        let summarizer = MemorySummarizer::new(SummarizationConfig {
            strategy: SummarizationStrategy::Incremental,
            ..Default::default()
        })
        .with_llm(Arc::new(MockLlm));

        let ctx = LsContext::with_session(lingshu_core::LsId::new());

        let base_summary = MemorySummary::new(
            "s1",
            "**摘要**：历史摘要内容。\n**核心话题**：历史话题".to_string(),
            Utc::now() - chrono::Duration::hours(1),
            Utc::now() - chrono::Duration::minutes(30),
            10,
            None,
            SummarizationStrategy::Rewrite,
        );

        let new_items = vec![
            MemoryItem::new("s1", "user", "我们继续讨论"),
            MemoryItem::new("s1", "assistant", "好的，继续。"),
        ];

        let result = summarizer
            .summarize(&ctx, "s1", &new_items, Some(&base_summary))
            .await
            .unwrap();
        assert!(result.is_some());
        let summary = result.unwrap();
        assert_eq!(summary.strategy, SummarizationStrategy::Incremental);
        assert_eq!(summary.original_count, 2);
        assert!(summary.base_summary.is_some());
    }
}
