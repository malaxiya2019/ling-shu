//! DefaultAgent — 默认 Agent 实现.
//!
//! 基于 ReAct (Thought → Action → Observation) 循环的通用 Agent.
//! 集成 LLM、ToolRegistry、Memory 三大核心组件。
//!
//! ## 工作流程
//! 1. 接收用户输入
//! 2. LLM 生成思考 + 行动 (Tool call)
//! 3. 执行工具 → 观察结果
//! 4. 循环直到 LLM 给出最终回答
//! 5. 可选: 将对话写入 Memory

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::agent::{Agent, AgentOutput, AgentSnapshot, AgentStatus};
use lingshu_traits::llm::{Llm, LlmMessage, LlmRequest, LlmRole};
use lingshu_traits::memory::{Memory, MemoryItem};

use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// 默认 Agent 配置.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// 最大 ReAct 循环次数.
    pub max_iterations: u32,
    /// 使用的 LLM 模型名.
    pub model: String,
    /// LLM temperature.
    pub temperature: f64,
    /// 最大输出 tokens.
    pub max_tokens: u32,
    /// 是否启用记忆读写.
    pub enable_memory: bool,
    /// 系统提示词模板.
    pub system_prompt: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            model: "gpt-4o".into(),
            temperature: 0.7,
            max_tokens: 4096,
            enable_memory: true,
            system_prompt: DEFAULT_SYSTEM_PROMPT.into(),
        }
    }
}

const DEFAULT_SYSTEM_PROMPT: &str = r#"你是 LingShu AI 助手，一个功能强大的智能体系统。

## 能力
- 你可以使用各种工具来完成用户的任务
- 每次回复请先思考，再决定是否需要使用工具
- 使用工具时，以 JSON 格式返回工具调用

## 工作流程
1. **思考**: 分析用户需求，决定下一步行动
2. **行动**: 如果需要工具，输出 tool_call
3. **观察**: 等待工具执行结果
4. **回答**: 根据观察结果给出最终答复

## 输出格式
- 思考过程: 以 `Thought: ` 开头
- 最终回答: 直接回复用户

不需要工具时，直接给出回答即可。"#;

/// Agent 运行时状态.
#[derive(Debug)]
struct AgentState {
    status: AgentStatus,
    messages: Vec<LlmMessage>,
    current_iteration: u32,
}

/// 默认 Agent 实现.
pub struct DefaultAgent {
    id: LsId,
    config: AgentConfig,
    llm: Arc<dyn Llm>,
    tools: Arc<tokio::sync::RwLock<lingshu_runtime::ToolRegistry>>,
    memory: Option<Arc<dyn Memory>>,
    state: Mutex<AgentState>,
    cancelled: AtomicBool,
    paused: AtomicBool,
}

impl DefaultAgent {
    /// 创建默认 Agent.
    pub fn new(
        config: AgentConfig,
        llm: Arc<dyn Llm>,
        tools: Arc<tokio::sync::RwLock<lingshu_runtime::ToolRegistry>>,
        memory: Option<Arc<dyn Memory>>,
    ) -> Self {
        Self {
            id: LsId::new(),
            config,
            llm,
            tools,
            memory,
            state: Mutex::new(AgentState {
                status: AgentStatus::Idle,
                messages: Vec::new(),
                current_iteration: 0,
            }),
            cancelled: AtomicBool::new(false),
            paused: AtomicBool::new(false),
        }
    }

    /// 构建 LLM 请求.
    fn build_request(&self, messages: &[LlmMessage]) -> LlmRequest {
        LlmRequest {
            model: self.config.model.clone(),
            messages: messages.to_vec(),
            temperature: Some(self.config.temperature),
            max_tokens: Some(self.config.max_tokens),
            tools: None, // Will be populated by the runtime
            stream: false,
        }
    }

    /// 将工具调用结果转为 LLM 消息.
    fn tool_result_to_message(_tool_call_id: String, content: String) -> LlmMessage {
        LlmMessage {
            role: LlmRole::Tool,
            content,
            name: None,
            content_parts: None,
            tool_calls: None,
        }
    }

    /// 将观察结果写入记忆 (如果启用).
    async fn write_to_memory(&self, ctx: &LsContext, role: &str, content: &str) {
        if let Some(ref memory) = self.memory {
            if self.config.enable_memory {
                let item = MemoryItem {
                    memory_id: LsId::new(),
                    session_id: ctx.session_id,
                    content: Value::String(format!("[{role}] {content}")),
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("agent_id".into(), self.id.to_string());
                        m.insert("role".into(), role.into());
                        m
                    },
                    created_at: chrono::Utc::now(),
                    ttl_seconds: Some(86400 * 30), // 30 days
                };
                if let Err(e) = memory.write(ctx.child(), item).await {
                    debug!(error = %e, "failed to write to memory");
                }
            }
        }
    }
}

#[async_trait]
impl Agent for DefaultAgent {
    fn id(&self) -> LsId {
        self.id
    }

    async fn run(&mut self, ctx: LsContext, input: Value) -> LsResult<AgentOutput> {
        info!(agent_id = %self.id, "agent run started");

        let input_text = match &input {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        // Reset state
        {
            let mut state = self.state.lock().await;
            state.status = AgentStatus::Running;
            state.current_iteration = 0;
            state.messages = vec![
                LlmMessage {
                    role: LlmRole::System,
                    content: self.config.system_prompt.clone(),
                    name: None,
                    content_parts: None,
                    tool_calls: None,
                },
                LlmMessage {
                    role: LlmRole::User,
                    content: input_text.clone(),
                    name: None,
                    content_parts: None,
                    tool_calls: None,
                },
            ];
        }

        self.cancelled.store(false, Ordering::Release);
        self.paused.store(false, Ordering::Release);

        // Write user input to memory
        self.write_to_memory(&ctx, "user", &input_text).await;

        // ReAct loop
        loop {
            // Check cancellation
            if self.cancelled.load(Ordering::Acquire) {
                let mut state = self.state.lock().await;
                state.status = AgentStatus::Completed;
                return Ok(AgentOutput {
                    agent_id: self.id,
                    status: AgentStatus::Completed,
                    data: Some(Value::String("Agent execution cancelled".into())),
                    error: None,
                });
            }

            // Check pause
            while self.paused.load(Ordering::Acquire) {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }

            // Check iteration limit
            {
                let state = self.state.lock().await;
                if state.current_iteration >= self.config.max_iterations {
                    let mut state = self.state.lock().await;
                    state.status = AgentStatus::Completed;
                    return Ok(AgentOutput {
                        agent_id: self.id,
                        status: AgentStatus::Completed,
                        data: Some(Value::String(format!(
                            "Exceeded max iterations ({})",
                            self.config.max_iterations
                        ))),
                        error: None,
                    });
                }
            }

            // Increment iteration
            {
                let mut state = self.state.lock().await;
                state.current_iteration += 1;
            }

            // Get tool definitions
            let tool_defs = {
                let registry = self.tools.read().await;
                registry.get_tool_definitions().await
            };

            // Build request with tools
            let mut request = self.build_request(&{
                let state = self.state.lock().await;
                state.messages.clone()
            });
            if !tool_defs.is_empty() {
                request.tools = Some(tool_defs);
            }

            // Call LLM
            let response = match self.llm.invoke(ctx.child(), request).await {
                Ok(r) => r,
                Err(e) => {
                    warn!(error = %e, "LLM call failed");
                    let mut state = self.state.lock().await;
                    state.status = AgentStatus::Failed;
                    return Ok(AgentOutput {
                        agent_id: self.id,
                        status: AgentStatus::Failed,
                        data: None,
                        error: Some(LsError::Llm(format!("LLM call failed: {e}"))),
                    });
                }
            };

            // Add assistant message to history
            {
                let mut state = self.state.lock().await;
                state.messages.push(response.message.clone());
            }

            // Write assistant response to memory
            self.write_to_memory(&ctx, "assistant", &response.message.content)
                .await;

            // Check if there are tool calls
            if let Some(tool_calls) = &response.message.tool_calls {
                if !tool_calls.is_empty() {
                    // Process each tool call
                    for tool_call in tool_calls {
                        // Parse arguments
                        let args: Value = match serde_json::from_str(&tool_call.function.arguments)
                        {
                            Ok(v) => v,
                            Err(e) => {
                                let error_msg = format!("Failed to parse arguments: {e}");
                                let tool_msg =
                                    Self::tool_result_to_message(tool_call.id.clone(), error_msg);
                                let mut state = self.state.lock().await;
                                state.messages.push(tool_msg);
                                continue;
                            }
                        };

                        // Execute tool
                        let result = {
                            let registry = self.tools.read().await;
                            registry
                                .execute(&ctx, &tool_call.function.name, args, None)
                                .await
                        };

                        let result_content = match result {
                            Ok(val) => serde_json::to_string_pretty(&val)
                                .unwrap_or_else(|_| "{}".to_string()),
                            Err(e) => format!("Tool execution error: {e}"),
                        };

                        // Add tool result to messages
                        let tool_msg =
                            Self::tool_result_to_message(tool_call.id.clone(), result_content);
                        {
                            let mut state = self.state.lock().await;
                            state.messages.push(tool_msg);
                        }
                    }
                    // Continue loop to let LLM process results
                    continue;
                }
            }

            // No tool calls — final answer
            let final_content = response.message.content.clone();
            {
                let mut state = self.state.lock().await;
                state.status = AgentStatus::Completed;
            }

            info!(agent_id = %self.id, "agent run completed");
            return Ok(AgentOutput {
                agent_id: self.id,
                status: AgentStatus::Completed,
                data: Some(Value::String(final_content)),
                error: None,
            });
        }
    }

    async fn pause(&mut self, _ctx: LsContext) -> LsResult<()> {
        self.paused.store(true, Ordering::Release);
        {
            let mut state = self.state.lock().await;
            state.status = AgentStatus::Paused;
        }
        info!(agent_id = %self.id, "agent paused");
        Ok(())
    }

    async fn resume(&mut self, _ctx: LsContext) -> LsResult<()> {
        self.paused.store(false, Ordering::Release);
        {
            let mut state = self.state.lock().await;
            state.status = AgentStatus::Running;
        }
        info!(agent_id = %self.id, "agent resumed");
        Ok(())
    }

    async fn cancel(&mut self, _ctx: LsContext) -> LsResult<()> {
        self.cancelled.store(true, Ordering::Release);
        {
            let mut state = self.state.lock().await;
            state.status = AgentStatus::Completed;
        }
        info!(agent_id = %self.id, "agent cancelled");
        Ok(())
    }

    async fn snapshot(&self, _ctx: LsContext) -> LsResult<AgentSnapshot> {
        let state = self.state.lock().await;
        let state_bytes = serde_json::to_vec(&state.messages)
            .map_err(|e| LsError::Internal(format!("failed to serialize agent state: {e}")))?;

        Ok(AgentSnapshot {
            agent_id: self.id,
            status: state.status,
            context: LsContext::with_session(LsId::new()),
            state: state_bytes,
            created_at: chrono::Utc::now(),
        })
    }

    async fn restore(&mut self, _ctx: LsContext, snapshot: AgentSnapshot) -> LsResult<()> {
        let messages: Vec<LlmMessage> = serde_json::from_slice(&snapshot.state)
            .map_err(|e| LsError::Internal(format!("failed to deserialize agent state: {e}")))?;

        let mut state = self.state.lock().await;
        state.messages = messages;
        state.status = snapshot.status;
        info!(agent_id = %self.id, "agent restored from snapshot");
        Ok(())
    }

    async fn status(&self, _ctx: LsContext) -> LsResult<AgentStatus> {
        let state = self.state.lock().await;
        Ok(state.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use lingshu_runtime::ToolRegistry;
    use lingshu_traits::agent::AgentStatus;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_agent_initial_state() {
        // Just verify creation - full tests below
    }

    fn make_agent() -> DefaultAgent {
        let registry = Arc::new(tokio::sync::RwLock::new(ToolRegistry::new()));

        DefaultAgent::new(
            AgentConfig::default(),
            Arc::new(crate::MockLlm::new()),
            registry,
            None,
        )
    }

    #[tokio::test]
    async fn test_agent_pause_resume_cancel() {
        let mut agent = make_agent();
        let ctx = LsContext::with_session(LsId::new());

        assert_eq!(agent.status(ctx.child()).await.unwrap(), AgentStatus::Idle);

        agent.pause(ctx.child()).await.unwrap();
        assert_eq!(
            agent.status(ctx.child()).await.unwrap(),
            AgentStatus::Paused
        );

        agent.resume(ctx.child()).await.unwrap();
        assert_eq!(
            agent.status(ctx.child()).await.unwrap(),
            AgentStatus::Running
        );

        agent.cancel(ctx.child()).await.unwrap();
        assert_eq!(
            agent.status(ctx.child()).await.unwrap(),
            AgentStatus::Completed
        );
    }

    #[tokio::test]
    async fn test_agent_snapshot_restore() {
        let mut agent = make_agent();
        let ctx = LsContext::with_session(LsId::new());

        let snap = agent.snapshot(ctx.child()).await.unwrap();
        assert_eq!(snap.agent_id, agent.id());

        agent.restore(ctx.child(), snap).await.unwrap();
        assert_eq!(agent.status(ctx.child()).await.unwrap(), AgentStatus::Idle);
    }
}
