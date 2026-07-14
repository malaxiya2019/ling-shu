//! GenAI 追踪 — Monocle 风格 OpenTelemetry span 属性。
//!
//! 遵循 Monocle GenAI Metamodel 的 span 属性约定，
//! 为 LLM 调用、Agent 执行、工具调用提供标准化可观测性。
//!
//! ## Span 属性命名规范
//! - `gen_ai.operation.name` — 操作名称
//! - `gen_ai.request.model` — 模型 ID
//! - `gen_ai.usage.input_tokens` — 输入 token 数
//! - `gen_ai.usage.output_tokens` — 输出 token 数
//! - `gen_ai.agent.id` — Agent session ID
//! - `gen_ai.tool.name` — 工具名称

use lingshu_core::LsContext;

/// GenAI 操作类型.
#[derive(Debug, Clone, Copy)]
pub enum GenAiOperation {
    Chat,
    ChatStream,
    Embed,
    AgentRun,
    ToolCall,
}

impl std::fmt::Display for GenAiOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenAiOperation::Chat => write!(f, "chat"),
            GenAiOperation::ChatStream => write!(f, "chat.stream"),
            GenAiOperation::Embed => write!(f, "embed"),
            GenAiOperation::AgentRun => write!(f, "agent.run"),
            GenAiOperation::ToolCall => write!(f, "tool.call"),
        }
    }
}

/// 创建标准化的 LLM tracing span。
///
/// 返回的 span 包含:
/// - `gen_ai.operation.name` — 操作类型
/// - `gen_ai.request.model` — 模型名
/// - `trace_id` / `session_id` — 从 LsContext 提取
/// - 自定义附加字段
#[macro_export]
macro_rules! genai_span {
    ($op:expr, $model:expr, $ctx:expr) => {{
        let op: &str = &$op.to_string();
        let model: &str = $model;
        let ctx: &LsContext = &$ctx;
        tracing::info_span!(
            "gen_ai",
            gen_ai.operation.name = op,
            gen_ai.request.model = model,
            trace_id = %ctx.trace_id,
            session_id = %ctx.session_id,
            user_id = %ctx.user_id.as_deref().unwrap_or("unknown"),
        )
    }};
}

/// 在 span 中记录 LLM 用量信息。
pub fn record_usage(span: &tracing::Span, input_tokens: u64, output_tokens: u64) {
    span.record("gen_ai.usage.input_tokens", input_tokens);
    span.record("gen_ai.usage.output_tokens", output_tokens);
}

/// 在 span 中记录工具调用信息。
pub fn record_tool_call(span: &tracing::Span, tool_name: &str, tool_call_id: &str) {
    span.record("gen_ai.tool.name", tool_name);
    span.record("gen_ai.tool.call_id", tool_call_id);
}

/// 在 span 中记录 Agent 信息。
pub fn record_agent(span: &tracing::Span, agent_id: &str, agent_name: &str) {
    span.record("gen_ai.agent.id", agent_id);
    span.record("gen_ai.agent.name", agent_name);
}

/// 创建用于 LLM invoke 的 span，执行后记录用量。
pub async fn traced_invoke<F, T>(ctx: LsContext, model: &str, operation: GenAiOperation, f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let span = genai_span!(operation, model, ctx);
    let _guard = span.enter();
    let start = std::time::Instant::now();
    let result = f.await;
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    tracing::debug!(duration_ms, gen_ai.operation.name = %operation, "LLM call completed");
    result
}
