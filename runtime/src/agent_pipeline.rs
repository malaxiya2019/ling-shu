//! AgentPipeline — 可配置的 Agent 执行流水线.
//!
//! 将 Agent 的执行过程分解为多个可插拔的阶段:
//!
//! ```text
//! Input → PreProcess → Think(LLM) → Act(Tools) → Observe → PostProcess → Memory → Output
//!    ↑                                                                          │
//!    └────────────────────────── ReAct 循环 ──────────────────────────────────────┘
//! ```
//!
//! 每个阶段都可以独立实现和替换，支持自定义 Agent 行为。

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::agent::AgentOutput;
use lingshu_traits::llm::{Llm, LlmMessage, LlmRole};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// 流水线上下文 — 在执行过程中传递.
#[derive(Debug, Clone)]
pub struct PipelineContext {
    /// Agent ID.
    pub agent_id: LsId,
    /// Agent 名称.
    pub agent_name: String,
    /// 用户输入.
    pub input: Value,
    /// 对话历史.
    pub messages: Vec<LlmMessage>,
    /// 当前迭代次数.
    pub iteration: u32,
    /// 最大迭代次数.
    pub max_iterations: u32,
    /// 额外元数据.
    pub metadata: std::collections::HashMap<String, String>,
}

impl PipelineContext {
    /// 创建新的流水线上下文.
    pub fn new(agent_id: LsId, agent_name: String, input: Value) -> Self {
        Self {
            agent_id,
            agent_name,
            input,
            messages: Vec::new(),
            iteration: 0,
            max_iterations: 10,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// 增加迭代计数.
    pub fn increment_iteration(&mut self) {
        self.iteration += 1;
    }

    /// 是否达到最大迭代次数.
    pub fn is_max_iterations_reached(&self) -> bool {
        self.iteration >= self.max_iterations
    }
}

/// 流水线阶段结果.
#[derive(Debug)]
pub enum StageAction {
    /// 继续到下一阶段.
    Continue,
    /// 跳过后续阶段，直接返回输出.
    SkipToOutput(Value),
    /// 终止流水线执行.
    Terminate(String),
}

/// 流水线阶段 trait — 所有阶段必须实现.
#[async_trait]
pub trait PipelineStage: Send + Sync {
    /// 阶段名称.
    fn name(&self) -> &str;

    /// 执行阶段逻辑.
    async fn execute(
        &self,
        ctx: &LsContext,
        pipeline_ctx: &mut PipelineContext,
    ) -> LsResult<StageAction>;
}

// ── 预定义阶段 ──

/// 预处理阶段 — 标准化用户输入.
pub struct PreProcessStage;

#[async_trait]
impl PipelineStage for PreProcessStage {
    fn name(&self) -> &str {
        "pre_process"
    }

    async fn execute(
        &self,
        _ctx: &LsContext,
        pipeline_ctx: &mut PipelineContext,
    ) -> LsResult<StageAction> {
        debug!("pre_process: normalizing input");

        let input_text = match &pipeline_ctx.input {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        // Add system message if not present
        if !pipeline_ctx
            .messages
            .iter()
            .any(|m| m.role == LlmRole::System)
        {
            pipeline_ctx.messages.push(LlmMessage {
                role: LlmRole::System,
                content: DEFAULT_SYSTEM_PROMPT.to_string(),
                name: None,
                content_parts: None,
                tool_calls: None,
            });
        }

        // Add user message
        pipeline_ctx.messages.push(LlmMessage {
            role: LlmRole::User,
            content: input_text,
            name: None,
            content_parts: None,
            tool_calls: None,
        });

        Ok(StageAction::Continue)
    }
}

/// LLM 思考阶段 — 调用 LLM 生成响应.
pub struct ThinkStage {
    llm: Arc<dyn Llm>,
    model: String,
    temperature: f64,
    max_tokens: u32,
}

impl ThinkStage {
    pub fn new(llm: Arc<dyn Llm>, model: impl Into<String>) -> Self {
        Self {
            llm,
            model: model.into(),
            temperature: 0.7,
            max_tokens: 4096,
        }
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

#[async_trait]
impl PipelineStage for ThinkStage {
    fn name(&self) -> &str {
        "think"
    }

    async fn execute(
        &self,
        ctx: &LsContext,
        pipeline_ctx: &mut PipelineContext,
    ) -> LsResult<StageAction> {
        debug!(iteration = pipeline_ctx.iteration, "think: calling LLM");

        let request = lingshu_traits::llm::LlmRequest {
            model: self.model.clone(),
            messages: pipeline_ctx.messages.clone(),
            temperature: Some(self.temperature),
            max_tokens: Some(self.max_tokens),
            tools: None, // Tools are injected by the ActStage
            stream: false,
        };

        match self.llm.invoke(ctx.child(), request).await {
            Ok(response) => {
                pipeline_ctx.messages.push(response.message.clone());
                Ok(StageAction::Continue)
            }
            Err(e) => {
                warn!(error = %e, "LLM call failed in think stage");
                Err(LsError::Llm(format!("LLM call failed: {e}")))
            }
        }
    }
}

/// 工具执行阶段 — 解析 LLM 输出中的工具调用并执行.
pub struct ActStage {
    tool_registry: Arc<tokio::sync::RwLock<lingshu_tool::ToolRegistry>>,
}

impl ActStage {
    pub fn new(tool_registry: Arc<tokio::sync::RwLock<lingshu_tool::ToolRegistry>>) -> Self {
        Self { tool_registry }
    }
}

#[async_trait]
impl PipelineStage for ActStage {
    fn name(&self) -> &str {
        "act"
    }

    async fn execute(
        &self,
        ctx: &LsContext,
        pipeline_ctx: &mut PipelineContext,
    ) -> LsResult<StageAction> {
        // Get the last message
        let last_message = match pipeline_ctx.messages.last() {
            Some(msg) => msg,
            None => return Ok(StageAction::Continue),
        };

        // Check for tool calls
        let tool_calls = match &last_message.tool_calls {
            Some(calls) if !calls.is_empty() => calls.clone(),
            _ => return Ok(StageAction::Continue), // No tool calls, skip to output
        };

        debug!(tool_calls = tool_calls.len(), "act: executing tool calls");

        for tool_call in &tool_calls {
            let args: Value = match serde_json::from_str(&tool_call.function.arguments) {
                Ok(v) => v,
                Err(e) => {
                    let error_msg = format!("Failed to parse arguments: {e}");
                    pipeline_ctx.messages.push(LlmMessage {
                        role: LlmRole::Tool,
                        content: error_msg,
                        name: None,
                        content_parts: None,
                        tool_calls: None,
                    });
                    continue;
                }
            };

            let child_ctx = ctx.child();
            let result = {
                let registry = self.tool_registry.read().await;
                registry
                    .execute(&child_ctx, &tool_call.function.name, args, None)
                    .await
            };

            let result_content = match result {
                Ok(val) => serde_json::to_string_pretty(&val).unwrap_or_else(|_| "{}".to_string()),
                Err(e) => format!("Tool execution error: {e}"),
            };

            pipeline_ctx.messages.push(LlmMessage {
                role: LlmRole::Tool,
                content: result_content,
                name: None,
                content_parts: None,
                tool_calls: None,
            });
        }

        // Continue the loop to process tool results
        Ok(StageAction::Continue)
    }
}

/// 后处理阶段 — 提取最终输出.
pub struct PostProcessStage;

#[async_trait]
impl PipelineStage for PostProcessStage {
    fn name(&self) -> &str {
        "post_process"
    }

    async fn execute(
        &self,
        _ctx: &LsContext,
        pipeline_ctx: &mut PipelineContext,
    ) -> LsResult<StageAction> {
        debug!("post_process: extracting final output");

        // Get the last assistant message as output
        for msg in pipeline_ctx.messages.iter().rev() {
            if msg.role == LlmRole::Assistant {
                // Check if this message has tool calls — if so, not final
                let has_tool_calls = msg.tool_calls.as_ref().is_some_and(|c| !c.is_empty());
                if !has_tool_calls {
                    return Ok(StageAction::SkipToOutput(Value::String(
                        msg.content.clone(),
                    )));
                }
            }
        }

        // No final message found, continue loop
        Ok(StageAction::Continue)
    }
}

/// 记忆存储阶段 — 将对话写入记忆.
pub struct MemoryStage {
    memory: Option<Arc<dyn lingshu_traits::memory::Memory>>,
}

impl MemoryStage {
    pub fn new(memory: Option<Arc<dyn lingshu_traits::memory::Memory>>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl PipelineStage for MemoryStage {
    fn name(&self) -> &str {
        "memory"
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

        debug!("memory: writing conversation to memory");

        // Write all messages to memory
        for msg in &pipeline_ctx.messages {
            let role_str = match msg.role {
                LlmRole::System => "system",
                LlmRole::User => "user",
                LlmRole::Assistant => "assistant",
                LlmRole::Tool => "tool",
                // All variants covered above
            };

            let item = lingshu_traits::memory::MemoryItem {
                memory_id: LsId::new(),
                session_id: ctx.session_id,
                content: Value::String(format!("[{role_str}] {}", msg.content)),
                metadata: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("agent_id".into(), pipeline_ctx.agent_id.to_string());
                    m.insert("role".into(), role_str.into());
                    m.insert("agent_name".into(), pipeline_ctx.agent_name.clone());
                    m
                },
                created_at: chrono::Utc::now(),
                ttl_seconds: Some(86400 * 30),
            };

            if let Err(e) = memory.write(ctx.child(), item).await {
                debug!(error = %e, "failed to write to memory");
            }
        }

        Ok(StageAction::Continue)
    }
}

// ── AgentPipeline ──

/// Agent 执行流水线.
pub struct AgentPipeline {
    /// 执行阶段列表（按顺序执行）.
    stages: Vec<Box<dyn PipelineStage>>,
    /// 最大 ReAct 循环次数.
    max_iterations: u32,
}

impl AgentPipeline {
    /// 创建空的流水线.
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            max_iterations: 10,
        }
    }

    /// 创建默认的 ReAct 流水线.
    pub fn default_react(
        llm: Arc<dyn Llm>,
        model: impl Into<String>,
        tool_registry: Arc<tokio::sync::RwLock<lingshu_tool::ToolRegistry>>,
        memory: Option<Arc<dyn lingshu_traits::memory::Memory>>,
    ) -> Self {
        let mut pipeline = Self::new();
        pipeline.add_stage(PreProcessStage);
        pipeline.add_stage(ThinkStage::new(llm, model));
        pipeline.add_stage(ActStage::new(tool_registry));
        pipeline.add_stage(PostProcessStage);
        pipeline.add_stage(MemoryStage::new(memory));
        pipeline
    }

    /// 添加流水线阶段.
    pub fn add_stage<S: PipelineStage + 'static>(&mut self, stage: S) {
        self.stages.push(Box::new(stage));
    }

    /// 设置最大迭代次数.
    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    /// 执行流水线.
    pub async fn execute(
        &self,
        ctx: LsContext,
        agent_id: LsId,
        agent_name: String,
        input: Value,
    ) -> LsResult<AgentOutput> {
        let mut pipeline_ctx = PipelineContext::new(agent_id, agent_name.clone(), input.clone());
        pipeline_ctx.max_iterations = self.max_iterations;

        info!(
            agent = %agent_name,
            stages = self.stages.len(),
            max_iterations = self.max_iterations,
            "pipeline execution started"
        );

        // ReAct loop
        loop {
            // Check iteration limit
            if pipeline_ctx.is_max_iterations_reached() {
                info!(
                    agent = %agent_name,
                    iteration = pipeline_ctx.iteration,
                    "max iterations reached"
                );
                return Ok(AgentOutput {
                    agent_id: pipeline_ctx.agent_id,
                    status: lingshu_traits::agent::AgentStatus::Completed,
                    data: Some(Value::String(format!(
                        "Exceeded max iterations ({})",
                        self.max_iterations
                    ))),
                    error: None,
                });
            }

            pipeline_ctx.increment_iteration();
            debug!(
                agent = %agent_name,
                iteration = pipeline_ctx.iteration,
                "pipeline iteration"
            );

            // Execute each stage
            let mut last_output = None;
            let mut should_terminate = false;
            let mut terminate_reason = String::new();

            'stages: for stage in &self.stages {
                match stage.execute(&ctx, &mut pipeline_ctx).await? {
                    StageAction::Continue => continue,
                    StageAction::SkipToOutput(output) => {
                        last_output = Some(output);
                        break 'stages;
                    }
                    StageAction::Terminate(reason) => {
                        should_terminate = true;
                        terminate_reason = reason;
                        break 'stages;
                    }
                }
            }

            if should_terminate {
                return Ok(AgentOutput {
                    agent_id: pipeline_ctx.agent_id,
                    status: lingshu_traits::agent::AgentStatus::Completed,
                    data: Some(Value::String(terminate_reason)),
                    error: None,
                });
            }

            if let Some(output) = last_output {
                info!(
                    agent = %agent_name,
                    iteration = pipeline_ctx.iteration,
                    "agent execution completed"
                );
                return Ok(AgentOutput {
                    agent_id: pipeline_ctx.agent_id,
                    status: lingshu_traits::agent::AgentStatus::Completed,
                    data: Some(output),
                    error: None,
                });
            }

            // Check if last message has tool calls — if not, we have final output
            if let Some(last_msg) = pipeline_ctx.messages.last() {
                let has_tool_calls = last_msg.tool_calls.as_ref().is_some_and(|c| !c.is_empty());
                if !has_tool_calls && last_msg.role == LlmRole::Assistant {
                    return Ok(AgentOutput {
                        agent_id: pipeline_ctx.agent_id,
                        status: lingshu_traits::agent::AgentStatus::Completed,
                        data: Some(Value::String(last_msg.content.clone())),
                        error: None,
                    });
                }
            }
        }
    }
}

impl Default for AgentPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// 默认系统提示词.
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"你是 LingShu AI 助手，一个功能强大的智能体系统。

## 能力
- 你可以使用各种工具来完成用户的任务
- 每次回复请先思考，再决定是否需要使用工具
- 使用工具时，以 JSON 格式返回工具调用

## 工作流程
1. **思考**: 分析用户需求，决定下一步行动
2. **行动**: 如果需要工具，输出 tool_call
3. **观察**: 等待工具执行结果
4. **回答**: 根据观察结果给出最终答复

不需要工具时，直接给出回答即可。"#;

/// 流水线 Agent — 使用 AgentPipeline 的 Agent 实现.
pub struct PipelineAgent {
    id: LsId,
    name: String,
    pipeline: Arc<AgentPipeline>,
    status: std::sync::RwLock<lingshu_traits::agent::AgentStatus>,
}

impl PipelineAgent {
    /// 创建新的 PipelineAgent.
    pub fn new(id: LsId, name: impl Into<String>, pipeline: Arc<AgentPipeline>) -> Self {
        Self {
            id,
            name: name.into(),
            pipeline,
            status: std::sync::RwLock::new(lingshu_traits::agent::AgentStatus::Idle),
        }
    }
}

#[async_trait]
impl lingshu_traits::agent::Agent for PipelineAgent {
    fn id(&self) -> LsId {
        self.id
    }

    async fn run(&mut self, ctx: LsContext, input: Value) -> LsResult<AgentOutput> {
        {
            let mut status = self
                .status
                .write()
                .map_err(|e| LsError::Internal(format!("status lock poisoned: {e}")))?;
            *status = lingshu_traits::agent::AgentStatus::Running;
        }

        let result = self
            .pipeline
            .execute(ctx, self.id, self.name.clone(), input)
            .await;

        {
            let mut status = self
                .status
                .write()
                .map_err(|e| LsError::Internal(format!("status lock poisoned: {e}")))?;
            *status = lingshu_traits::agent::AgentStatus::Completed;
        }

        result
    }

    async fn pause(&mut self, _ctx: LsContext) -> LsResult<()> {
        let mut status = self
            .status
            .write()
            .map_err(|e| LsError::Internal(format!("status lock poisoned: {e}")))?;
        *status = lingshu_traits::agent::AgentStatus::Paused;
        Ok(())
    }

    async fn resume(&mut self, _ctx: LsContext) -> LsResult<()> {
        let mut status = self
            .status
            .write()
            .map_err(|e| LsError::Internal(format!("status lock poisoned: {e}")))?;
        *status = lingshu_traits::agent::AgentStatus::Running;
        Ok(())
    }

    async fn cancel(&mut self, _ctx: LsContext) -> LsResult<()> {
        let mut status = self
            .status
            .write()
            .map_err(|e| LsError::Internal(format!("status lock poisoned: {e}")))?;
        *status = lingshu_traits::agent::AgentStatus::Completed;
        Ok(())
    }

    async fn snapshot(&self, _ctx: LsContext) -> LsResult<lingshu_traits::agent::AgentSnapshot> {
        let status = self
            .status
            .read()
            .map_err(|e| LsError::Internal(format!("status lock poisoned: {e}")))?;
        Ok(lingshu_traits::agent::AgentSnapshot {
            agent_id: self.id,
            status: *status,
            context: LsContext::with_session(LsId::new()),
            state: Vec::new(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn restore(
        &mut self,
        _ctx: LsContext,
        _snapshot: lingshu_traits::agent::AgentSnapshot,
    ) -> LsResult<()> {
        Ok(())
    }

    async fn status(&self, _ctx: LsContext) -> LsResult<lingshu_traits::agent::AgentStatus> {
        let status = self
            .status
            .read()
            .map_err(|e| LsError::Internal(format!("status lock poisoned: {e}")))?;
        Ok(*status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_create() {
        let pipeline = AgentPipeline::new();
        assert_eq!(pipeline.stages.len(), 0);
    }

    #[test]
    fn test_pipeline_context() {
        let ctx = PipelineContext::new(
            LsId::new(),
            "test-agent".into(),
            Value::String("hello".into()),
        );
        assert_eq!(ctx.iteration, 0);
        assert_eq!(ctx.max_iterations, 10);
        assert!(!ctx.is_max_iterations_reached());
    }

    #[test]
    fn test_pipeline_context_max_iterations() {
        let mut ctx = PipelineContext::new(LsId::new(), "test".into(), Value::Null);
        ctx.max_iterations = 3;
        ctx.iteration = 3;
        assert!(ctx.is_max_iterations_reached());
    }

    #[tokio::test]
    async fn test_pre_process_stage_adds_messages() {
        let stage = PreProcessStage;
        let ctx = LsContext::with_session(LsId::new());
        let mut pipeline_ctx =
            PipelineContext::new(LsId::new(), "test".into(), Value::String("hello".into()));

        let result = stage.execute(&ctx, &mut pipeline_ctx).await.unwrap();
        assert!(matches!(result, StageAction::Continue));

        // Should have system + user messages
        assert_eq!(pipeline_ctx.messages.len(), 2);
        assert_eq!(pipeline_ctx.messages[0].role, LlmRole::System);
        assert_eq!(pipeline_ctx.messages[1].role, LlmRole::User);
        assert_eq!(pipeline_ctx.messages[1].content, "hello");
    }

    #[tokio::test]
    async fn test_post_process_stage() {
        let stage = PostProcessStage;
        let ctx = LsContext::with_session(LsId::new());
        let mut pipeline_ctx = PipelineContext::new(LsId::new(), "test".into(), Value::Null);

        // Add an assistant message without tool calls
        pipeline_ctx.messages.push(LlmMessage {
            role: LlmRole::Assistant,
            content: "Final answer".to_string(),
            name: None,
            content_parts: None,
            tool_calls: None,
        });

        let result = stage.execute(&ctx, &mut pipeline_ctx).await.unwrap();
        match result {
            StageAction::SkipToOutput(val) => {
                assert_eq!(val, Value::String("Final answer".to_string()));
            }
            _ => panic!("Expected SkipToOutput"),
        }
    }

    #[tokio::test]
    async fn test_post_process_stage_with_tool_calls() {
        let stage = PostProcessStage;
        let ctx = LsContext::with_session(LsId::new());
        let mut pipeline_ctx = PipelineContext::new(LsId::new(), "test".into(), Value::Null);

        // Add assistant message WITH tool calls
        pipeline_ctx.messages.push(LlmMessage {
            role: LlmRole::Assistant,
            content: "Let me use a tool".to_string(),
            name: None,
            content_parts: None,
            tool_calls: Some(vec![lingshu_traits::llm::ToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: lingshu_traits::llm::ToolCallFunction {
                    name: "test_tool".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
        });

        // Should continue because there are tool calls
        let result = stage.execute(&ctx, &mut pipeline_ctx).await.unwrap();
        assert!(matches!(result, StageAction::Continue));
    }
}

#[cfg(feature = "memory-summarizer")]
// ── Summarizer MemoryStage ─────────────────────────
impl MemoryStage {
    /// 创建带 LLM 摘要的记忆存储阶段.
    ///
    /// 在执行记忆存储时，使用 `SummarizerLlm` 对对话内容进行智能摘要，
    /// 以减少存储空间并提取关键信息.
    ///
    /// # 参数
    ///
    /// * `memory` — 可选记忆后端
    /// * `summarizer` — LLM 摘要器，用于生成对话摘要
    /// * `max_chars_before_summary` — 触发摘要的 token 阈值
    pub fn new_with_summarizer(
        memory: Option<Arc<dyn lingshu_traits::memory::Memory>>,
        summarizer: Arc<dyn lingshu_memory::summarization::SummarizerLlm>,
        max_chars_before_summary: usize,
    ) -> Self {
        // 包装 memory 使之在写入前先执行摘要
        let wrapped =
            memory.map(|mem| {
                let inner: Arc<dyn lingshu_traits::memory::Memory> = Arc::new(
                    SummarizingMemory::new(mem, summarizer, max_chars_before_summary),
                );
                inner
            });
        Self { memory: wrapped }
    }
}
#[cfg(feature = "memory-summarizer")]
/// 带摘要功能的记忆包装器.
/// 在将内容写入下游 Memory 之前，先通过 SummarizerLlm 生成摘要。
/// 如果摘要成功，则写入摘要文本而非原始内容。
struct SummarizingMemory {
    inner: Arc<dyn lingshu_traits::memory::Memory>,
    summarizer: Arc<dyn lingshu_memory::summarization::SummarizerLlm>,
    max_chars_threshold: usize,
}

#[cfg(feature = "memory-summarizer")]
impl SummarizingMemory {
    fn new(
        inner: Arc<dyn lingshu_traits::memory::Memory>,
        summarizer: Arc<dyn lingshu_memory::summarization::SummarizerLlm>,
        max_chars_threshold: usize,
    ) -> Self {
        Self {
            inner,
            summarizer,
            max_chars_threshold,
        }
    }

    /// 估算文本的 token 数量（粗略估算：每中文字符~2 tokens，每英文单词~1.3 tokens）.
    fn estimate_chars(text: &str) -> usize {
        let char_count = text.chars().count();
        let ascii_count = text.chars().filter(|c| c.is_ascii()).count();
        let non_ascii = char_count.saturating_sub(ascii_count);
        // 非 ASCII（主要是中文）：~2 tokens/字符
        // ASCII：~0.25 tokens/字符（平均4字符/token）
        non_ascii * 2 + ascii_count / 4
    }
}

#[cfg(feature = "memory-summarizer")]
#[async_trait::async_trait]
impl lingshu_traits::memory::Memory for SummarizingMemory {
    async fn write(
        &self,
        ctx: lingshu_core::LsContext,
        item: lingshu_traits::memory::MemoryItem,
    ) -> lingshu_core::LsResult<lingshu_core::LsId> {
        // 检查是否需要摘要
        let content_str = match &item.content {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        let estimated = Self::estimate_chars(&content_str);
        let should_summarize = estimated > self.max_chars_threshold;

        let content_to_store = if should_summarize {
            tracing::debug!(
                estimated_chars = estimated,
                threshold = self.max_chars_threshold,
                "content exceeds threshold, summarizing before storage"
            );
            // 执行摘要
            let prompt = format!(
                "请用中文简洁总结以下内容（保留关键信息，不超过200字）：\n\n{}",
                if content_str.len() > 2000 {
                    &content_str[..2000]
                } else {
                    &content_str
                }
            );
            match self
                .summarizer
                .generate(&ctx.child(), &prompt, "gpt-4o-mini")
                .await
            {
                Ok(summary_text) => {
                    tracing::debug!(
                        "memory summarization succeeded ({} chars → {})",
                        content_str.len(),
                        summary_text.len()
                    );
                    serde_json::Value::String(summary_text)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "memory summarization failed, storing original");
                    item.content.clone()
                }
            }
        } else {
            item.content.clone()
        };

        let summarized_item = lingshu_traits::memory::MemoryItem {
            content: content_to_store,
            ..item
        };

        self.inner.write(ctx, summarized_item).await
    }

    async fn write_batch(
        &self,
        ctx: lingshu_core::LsContext,
        items: Vec<lingshu_traits::memory::MemoryItem>,
    ) -> lingshu_core::LsResult<Vec<lingshu_core::LsId>> {
        let mut ids = Vec::with_capacity(items.len());
        for item in items {
            ids.push(self.write(ctx.child(), item).await?);
        }
        Ok(ids)
    }

    async fn read(
        &self,
        ctx: lingshu_core::LsContext,
        memory_id: lingshu_core::LsId,
    ) -> lingshu_core::LsResult<lingshu_traits::memory::MemoryItem> {
        self.inner.read(ctx, memory_id).await
    }

    async fn search(
        &self,
        ctx: lingshu_core::LsContext,
        query: &str,
        limit: u64,
    ) -> lingshu_core::LsResult<lingshu_traits::memory::MemorySearchResult> {
        self.inner.search(ctx, query, limit).await
    }

    async fn delete(
        &self,
        ctx: lingshu_core::LsContext,
        memory_id: lingshu_core::LsId,
    ) -> lingshu_core::LsResult<()> {
        self.inner.delete(ctx, memory_id).await
    }

    async fn clean_expired(&self, ctx: lingshu_core::LsContext) -> lingshu_core::LsResult<u64> {
        self.inner.clean_expired(ctx).await
    }

    async fn clear_session(
        &self,
        ctx: lingshu_core::LsContext,
        session_id: lingshu_core::LsId,
    ) -> lingshu_core::LsResult<()> {
        self.inner.clear_session(ctx, session_id).await
    }
}
