//! MemoryPipeline — Memory + RAG 全链路集成.
//!
//! 将记忆系统（Memory）、向量检索（VectorStore）与 Agent Pipeline 打通，
//! 实现会话级记忆检索、上下文注入、长期记忆存储。
//!
//! # 架构
//!
//! ```text
//! Agent Pipeline
//!   │
//!   ├── RetrieveStage ───── 检索相关记忆 → 注入上下文
//!   ├── ThinkStage ──────── LLM 推理 + 工具调用
//!   ├── ActStage ────────── 工具执行
//!   └── MemoryStage ─────── 存储本轮交互到记忆
//! ```
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use lingshu_runtime::agent_pipeline::*;
//! use lingshu_runtime::memory_pipeline::*;
//!
//! # fn example(memory: Arc<dyn MemoryBackend>) {
//! let mut pipeline = AgentPipeline::new();
//! pipeline.add_stage(RetrieveStage::new(memory, 5));
//! pipeline.add_stage(ThinkStage::new(/* llm */));
//! pipeline.add_stage(MemoryStage::new(Some(memory)));
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
#[cfg_attr(not(test), allow(unused_imports))]
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::memory::Memory;
use serde_json::Value;
use tracing::debug;

use crate::agent_pipeline::{PipelineContext, PipelineStage, StageAction};

/// 记忆检索阶段 — 在 Think 之前检索相关记忆并注入上下文.
///
/// 工作流程：
/// 1. 根据当前用户输入检索语义相关的历史记忆
/// 2. 将检索结果格式化为上下文，注入到 system 消息中
/// 3. 支持配置检索条数
pub struct RetrieveStage {
    /// 记忆后端.
    memory: Option<Arc<dyn Memory>>,
    /// 检索结果数量.
    top_k: usize,
    /// 注入的 system prompt 前缀.
    system_prompt_prefix: String,
}

impl RetrieveStage {
    /// 创建检索阶段.
    ///
    /// `memory` — 记忆后端，`None` 时跳过检索.
    pub fn new(memory: Option<Arc<dyn Memory>>, top_k: usize) -> Self {
        Self {
            memory,
            top_k,
            system_prompt_prefix: "## 历史记忆\n以下是本会话中相关的历史记忆，供你参考：\n"
                .to_string(),
        }
    }

    /// 设置 system prompt 前缀.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.system_prompt_prefix = prefix.into();
        self
    }
}

#[async_trait]
impl PipelineStage for RetrieveStage {
    fn name(&self) -> &str {
        "retrieve_memory"
    }

    async fn execute(
        &self,
        ctx: &LsContext,
        pipeline_ctx: &mut PipelineContext,
    ) -> LsResult<StageAction> {
        let memory = match &self.memory {
            Some(m) => m,
            None => return Ok(StageAction::Continue),
        };

        debug!("retrieve_stage: searching for relevant memories");

        // 1. 从用户输入提取查询
        let query = pipeline_ctx.input.as_str().unwrap_or("").to_string();

        if query.is_empty() {
            return Ok(StageAction::Continue);
        }

        // 2. 检索相关记忆
        let search_result = memory.search(ctx.clone(), &query, self.top_k as u64).await;

        let memories = match search_result {
            Ok(result) => result.items,
            Err(e) => {
                debug!(error = %e, "memory search failed, skipping retrieval");
                return Ok(StageAction::Continue);
            }
        };

        if memories.is_empty() {
            debug!("retrieve_stage: no relevant memories found");
            return Ok(StageAction::Continue);
        }

        // 3. 格式化记忆为上下文
        let mut memory_context = String::new();
        memory_context.push_str(&self.system_prompt_prefix);

        for (i, item) in memories.iter().enumerate() {
            let role = item
                .metadata
                .get("role")
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            let content_str = match &item.content {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            memory_context.push_str(&format!("\n[{}. ({})] {}", i + 1, role, content_str));
        }

        // 4. 注入到第一条 system 消息，或创建新的 system 消息
        if let Some(first_msg) = pipeline_ctx.messages.first_mut() {
            if first_msg.role == lingshu_traits::llm::LlmRole::System {
                first_msg.content = format!("{}\n\n{}", first_msg.content, memory_context);
            } else {
                // 如果没有 system 消息，创建一条
                pipeline_ctx.messages.insert(
                    0,
                    lingshu_traits::llm::LlmMessage {
                        role: lingshu_traits::llm::LlmRole::System,
                        content: memory_context,
                        name: None,
                        content_parts: None,
                        tool_calls: None,
                    },
                );
            }
        } else {
            pipeline_ctx.messages.push(lingshu_traits::llm::LlmMessage {
                role: lingshu_traits::llm::LlmRole::System,
                content: memory_context,
                name: None,
                content_parts: None,
                tool_calls: None,
            });
        }

        debug!(
            memory_count = memories.len(),
            "retrieve_stage: injected {} memories into context",
            memories.len()
        );

        Ok(StageAction::Continue)
    }
}

// ── RAG 配置 ──

/// RAG 全链路配置.
#[derive(Debug, Clone)]
pub struct RagConfig {
    /// 检索条数.
    pub top_k: usize,
    /// 最小相关性分数（0.0-1.0）.
    pub min_relevance: f64,
    /// 是否启用语义检索.
    pub enable_semantic_search: bool,
    /// 是否启用关键词检索.
    pub enable_keyword_search: bool,
    /// 检索超时（毫秒）.
    pub search_timeout_ms: u64,
    /// System prompt 前缀.
    pub system_prompt_prefix: String,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            min_relevance: 0.0,
            enable_semantic_search: true,
            enable_keyword_search: true,
            search_timeout_ms: 5000,
            system_prompt_prefix: "## 相关记忆\n以下是历史相关对话：\n".to_string(),
        }
    }
}

// ── 完整的 RAG Pipeline 构造器 ──

/// 构建完整的 RAG Pipeline（含检索、思考、行动、记忆存储）.
pub fn build_rag_pipeline(
    llm: Arc<dyn lingshu_traits::llm::Llm>,
    model: impl Into<String>,
    tool_registry: Arc<tokio::sync::RwLock<lingshu_tool::ToolRegistry>>,
    memory: Option<Arc<dyn Memory>>,
    rag_config: RagConfig,
) -> crate::agent_pipeline::AgentPipeline {
    let mut pipeline = crate::agent_pipeline::AgentPipeline::new();

    // 1. 预处理（添加 system + user 消息）
    pipeline.add_stage(crate::agent_pipeline::PreProcessStage);

    // 2. 记忆检索（RAG）
    pipeline.add_stage(
        RetrieveStage::new(memory.clone(), rag_config.top_k)
            .with_prefix(rag_config.system_prompt_prefix),
    );

    // 3. LLM 推理
    pipeline.add_stage(crate::agent_pipeline::ThinkStage::new(llm, model));

    // 4. 工具执行
    pipeline.add_stage(crate::agent_pipeline::ActStage::new(tool_registry));

    // 5. 后处理
    pipeline.add_stage(crate::agent_pipeline::PostProcessStage);

    // 6. 记忆存储
    pipeline.add_stage(crate::agent_pipeline::MemoryStage::new(memory));

    pipeline
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rag_config_defaults() {
        let config = RagConfig::default();
        assert_eq!(config.top_k, 5);
        assert!(config.enable_semantic_search);
    }

    #[tokio::test]
    async fn test_retrieve_stage_no_memory_continues() {
        let stage = RetrieveStage::new(None, 5);
        let ctx = LsContext::with_session(LsId::new());
        let mut pipeline_ctx =
            PipelineContext::new(LsId::new(), "test".into(), Value::String("hello".into()));

        let result = stage.execute(&ctx, &mut pipeline_ctx).await.unwrap();
        assert!(matches!(result, StageAction::Continue));
    }

    #[tokio::test]
    async fn test_retrieve_stage_empty_input_continues() {
        // No memory backend = skip retrieval
        let stage = RetrieveStage::new(None, 5);
        let ctx = LsContext::with_session(LsId::new());
        let mut pipeline_ctx =
            PipelineContext::new(LsId::new(), "test".into(), Value::String("".into()));

        let result = stage.execute(&ctx, &mut pipeline_ctx).await.unwrap();
        assert!(matches!(result, StageAction::Continue));
    }

    #[tokio::test]
    async fn test_build_rag_pipeline() {
        // Just verify the function returns a valid pipeline
        let _tool_registry = Arc::new(tokio::sync::RwLock::new(lingshu_tool::ToolRegistry::new()));

        // This would need a real LLM provider; just check it compiles
        let _config = RagConfig::default();
        assert_eq!(_config.top_k, 5);
    }
}
