//! Agent Runtime MCP Tools — 将 AgentRuntime API 暴露为 MCP 工具.
//!
//! 需要启用 `agent-runtime` feature.

use std::sync::Arc;

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_runtime::agent_runtime::AgentRuntime;
use lingshu_traits::tool::{Tool, ToolInfo, ToolMetadata, ToolParam};
use serde_json::Value;

// ── Agent 管理工具 ──

/// 列出所有 Agent.
pub struct ListAgentsTool {
    runtime: Arc<AgentRuntime>,
}

impl ListAgentsTool {
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for ListAgentsTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "list_agents".into(),
            description: "列出所有注册的 Agent".into(),
            parameters: vec![],
            metadata: ToolMetadata {
                timeout_ms: Some(5000),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn validate(&self, _input: &Value) -> LsResult<()> {
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, _input: Value) -> LsResult<Value> {
        let agents = self.runtime.list_agents().await;
        let summaries: Vec<serde_json::Value> = agents
            .into_iter()
            .map(|a| {
                serde_json::json!({
                    "agent_id": a.agent_id.to_string(),
                    "name": a.name,
                    "status": format!("{:?}", a.status),
                    "created_at": a.created_at.to_rfc3339(),
                })
            })
            .collect();
        Ok(serde_json::json!({ "agents": summaries }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            runtime: self.runtime.clone(),
        })
    }
}

/// 获取 Agent 状态.
pub struct AgentStatusTool {
    runtime: Arc<AgentRuntime>,
}

impl AgentStatusTool {
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for AgentStatusTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "agent_status".into(),
            description: "查询指定 Agent 的状态".into(),
            parameters: vec![ToolParam {
                name: "agent_id".into(),
                description: "Agent ID".into(),
                required: true,
                param_type: "string".into(),
            }],
            metadata: ToolMetadata {
                timeout_ms: Some(5000),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if input.get("agent_id").and_then(|v| v.as_str()).is_none() {
            return Err(LsError::InvalidArgument("missing agent_id".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let agent_id = input["agent_id"].as_str().unwrap_or("");
        let id = LsId::from(uuid::Uuid::parse_str(agent_id).map_err(|e| {
            LsError::InvalidArgument(format!("invalid agent_id: {}", e))
        })?);
        let status = self.runtime.agent_status(&id).await?;
        Ok(serde_json::json!({
            "agent_id": id.to_string(),
            "status": format!("{:?}", status),
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            runtime: self.runtime.clone(),
        })
    }
}

/// 移除 Agent.
pub struct RemoveAgentTool {
    runtime: Arc<AgentRuntime>,
}

impl RemoveAgentTool {
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for RemoveAgentTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "remove_agent".into(),
            description: "移除指定 Agent".into(),
            parameters: vec![ToolParam {
                name: "agent_id".into(),
                description: "Agent ID".into(),
                required: true,
                param_type: "string".into(),
            }],
            metadata: ToolMetadata {
                timeout_ms: Some(5000),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if input.get("agent_id").and_then(|v| v.as_str()).is_none() {
            return Err(LsError::InvalidArgument("missing agent_id".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let agent_id = input["agent_id"].as_str().unwrap_or("");
        let id = LsId::from(uuid::Uuid::parse_str(agent_id).map_err(|e| {
            LsError::InvalidArgument(format!("invalid agent_id: {}", e))
        })?);
        self.runtime.remove_agent(&id).await?;
        Ok(serde_json::json!({ "removed": true }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            runtime: self.runtime.clone(),
        })
    }
}

// ── 会话管理工具 ──

/// 创建会话.
pub struct CreateSessionTool {
    runtime: Arc<AgentRuntime>,
}

impl CreateSessionTool {
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for CreateSessionTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "create_session".into(),
            description: "创建新的会话".into(),
            parameters: vec![],
            metadata: ToolMetadata {
                timeout_ms: Some(5000),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn validate(&self, _input: &Value) -> LsResult<()> {
        Ok(())
    }

    async fn execute(&self, ctx: LsContext, _input: Value) -> LsResult<Value> {
        self.runtime.create_session(&ctx).await?;
        Ok(serde_json::json!({
            "session_id": ctx.session_id.to_string(),
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            runtime: self.runtime.clone(),
        })
    }
}

// ── 工作流工具 ──

/// 列出工作流.
pub struct ListWorkflowsMcpTool {
    runtime: Arc<AgentRuntime>,
}

impl ListWorkflowsMcpTool {
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for ListWorkflowsMcpTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "list_workflows".into(),
            description: "列出所有注册的工作流".into(),
            parameters: vec![],
            metadata: ToolMetadata {
                timeout_ms: Some(5000),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn validate(&self, _input: &Value) -> LsResult<()> {
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, _input: Value) -> LsResult<Value> {
        let workflows = self.runtime.list_workflows().await;
        Ok(serde_json::json!({ "workflows": workflows }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            runtime: self.runtime.clone(),
        })
    }
}

/// 执行工作流.
pub struct ExecuteWorkflowMcpTool {
    runtime: Arc<AgentRuntime>,
}

impl ExecuteWorkflowMcpTool {
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for ExecuteWorkflowMcpTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "execute_workflow".into(),
            description: "执行指定的工作流".into(),
            parameters: vec![
                ToolParam {
                    name: "name".into(),
                    description: "工作流名称".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "input".into(),
                    description: "工作流输入 (JSON)".into(),
                    required: false,
                    param_type: "object".into(),
                },
            ],
            metadata: ToolMetadata {
                timeout_ms: Some(30000),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if input.get("name").and_then(|v| v.as_str()).is_none() {
            return Err(LsError::InvalidArgument("missing 'name' parameter".into()));
        }
        Ok(())
    }

    async fn execute(&self, ctx: LsContext, input: Value) -> LsResult<Value> {
        let name = input["name"].as_str().unwrap_or("");
        let workflow_input = input.get("input").cloned().unwrap_or(serde_json::json!({}));
        self.runtime
            .execute_workflow(name, ctx, workflow_input)
            .await
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            runtime: self.runtime.clone(),
        })
    }
}

// ── Runtime 工具 ──

/// 查询 Runtime 状态.
pub struct RuntimeStatusTool {
    runtime: Arc<AgentRuntime>,
}

impl RuntimeStatusTool {
    pub fn new(runtime: Arc<AgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for RuntimeStatusTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "runtime_status".into(),
            description: "查询 Agent Runtime 状态".into(),
            parameters: vec![],
            metadata: ToolMetadata {
                timeout_ms: Some(5000),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn validate(&self, _input: &Value) -> LsResult<()> {
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, _input: Value) -> LsResult<Value> {
        let state = self.runtime.lifecycle_state().await;
        let state_str = state.map(|s| format!("{:?}", s)).unwrap_or_else(|_| "Unknown".into());
        let agent_count = self.runtime.agent_count().await;
        let session_count = self.runtime.active_sessions().await;
        Ok(serde_json::json!({
            "state": state_str,
            "agent_count": agent_count,
            "session_count": session_count,
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            runtime: self.runtime.clone(),
        })
    }
}

// ── 工厂函数 ──

/// 创建所有 Agent Runtime MCP 工具.
pub fn create_agent_runtime_tools(runtime: Arc<AgentRuntime>) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ListAgentsTool::new(runtime.clone())),
        Arc::new(AgentStatusTool::new(runtime.clone())),
        Arc::new(RemoveAgentTool::new(runtime.clone())),
        Arc::new(CreateSessionTool::new(runtime.clone())),
        Arc::new(ListWorkflowsMcpTool::new(runtime.clone())),
        Arc::new(ExecuteWorkflowMcpTool::new(runtime.clone())),
        Arc::new(RuntimeStatusTool::new(runtime)),
    ]
}
