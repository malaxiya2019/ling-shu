//! RuntimeWorkflowAccess — 运行时工作流访问实现.
//!
//! 实现 agent_runtime::WorkflowAccess trait，提供轻量级工作流注册、
//! 执行与状态查询能力，无需依赖外部 DAG 引擎.
//!
//! # 架构
//!
//! ```text
//! RuntimeWorkflowAccess
//!   ├── registry: HashMap<String, WorkflowDef>   ── 已注册的工作流
//!   ├── handlers: HashMap<String, WorkflowHandler> ── 异步执行器
//!   └── status: HashMap<String, ExecutionStatus>   ── 执行状态
//! ```
//!
//! # 注册示例
//!
//! ```rust,ignore
//! let access = RuntimeWorkflowAccess::new();
//! access.register("analyze", serde_json::json!({
//!     "steps": ["think", "act", "summarize"]
//! })).await;
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::agent_runtime::WorkflowAccess;

/// 工作流定义（轻量级 JSON 模板）.
#[derive(Debug, Clone)]
pub struct WorkflowDef {
    /// 工作流名称.
    pub name: String,
    /// 工作流定义 JSON.
    pub definition: Value,
    /// 可选的元数据标签.
    pub tags: Vec<String>,
}

/// 执行状态.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionState {
    Pending,
    Running,
    Completed,
    Failed(String),
}

/// 执行记录.
#[derive(Debug, Clone)]
pub struct ExecutionRecord {
    pub state: ExecutionState,
    pub input: Value,
    pub output: Option<Value>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
}

/// 工作流异步处理器签名.
pub type WorkflowHandler = Arc<
    dyn Fn(
            String,    // workflow name
            LsContext, // execution context
            Value,     // input
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LsResult<Value>> + Send>>
        + Send
        + Sync,
>;

/// RuntimeWorkflowAccess — 运行时工作流访问实现.
///
/// 支持两种执行模式:
/// 1. **Handler 模式** — 注册异步闭包，支持复杂工作流逻辑
/// 2. **模板模式** — 注册 JSON 定义，直接返回定义作为执行结果
pub struct RuntimeWorkflowAccess {
    /// 已注册的工作流定义.
    registry: RwLock<HashMap<String, WorkflowDef>>,
    /// 已注册的异步处理器.
    handlers: RwLock<HashMap<String, WorkflowHandler>>,
    /// 执行记录.
    execution_log: RwLock<HashMap<String, Vec<ExecutionRecord>>>,
}

impl RuntimeWorkflowAccess {
    /// 创建新的运行时工作流访问.
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(HashMap::new()),
            handlers: RwLock::new(HashMap::new()),
            execution_log: RwLock::new(HashMap::new()),
        }
    }

    /// 注册一个 JSON 定义的工作流.
    pub async fn register(&self, name: &str, definition: Value) {
        let mut reg = self.registry.write().await;
        reg.insert(
            name.to_string(),
            WorkflowDef {
                name: name.to_string(),
                definition,
                tags: Vec::new(),
            },
        );
        info!(workflow = %name, "workflow registered");
    }

    /// 注册一个带异步处理器的工作流.
    pub async fn register_with_handler(
        &self,
        name: &str,
        definition: Value,
        handler: WorkflowHandler,
    ) {
        self.register(name, definition).await;
        let mut handlers = self.handlers.write().await;
        handlers.insert(name.to_string(), handler);
    }

    /// 注册一个自定义处理器（无 JSON 定义）.
    pub async fn register_handler(&self, name: &str, handler: WorkflowHandler) {
        let mut handlers = self.handlers.write().await;
        handlers.insert(name.to_string(), handler);
        info!(workflow = %name, "workflow handler registered");
    }

    /// 检查工作流是否存在.
    pub async fn exists(&self, name: &str) -> bool {
        let reg = self.registry.read().await;
        reg.contains_key(name)
    }

    /// 获取工作流定义.
    pub async fn get_definition(&self, name: &str) -> Option<Value> {
        let reg = self.registry.read().await;
        reg.get(name).map(|def| def.definition.clone())
    }

    /// 获取执行历史.
    pub async fn get_execution_history(&self, name: &str) -> Vec<ExecutionRecord> {
        let log = self.execution_log.read().await;
        log.get(name).cloned().unwrap_or_default()
    }
}

impl Default for RuntimeWorkflowAccess {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WorkflowAccess for RuntimeWorkflowAccess {
    /// 列出所有已注册的工作流.
    async fn list_workflows(&self) -> Vec<Value> {
        let reg = self.registry.read().await;
        reg.values()
            .map(|def| {
                serde_json::json!({
                    "name": def.name,
                    "definition": def.definition,
                    "tags": def.tags,
                })
            })
            .collect()
    }

    /// 执行工作流.
    ///
    /// 优先级:
    /// 1. 如果有注册的异步处理器，使用处理器执行
    /// 2. 否则返回工作流定义本身作为结果（模板模式）
    async fn execute_workflow(&self, name: &str, ctx: LsContext, input: Value) -> LsResult<Value> {
        let now_ms = chrono::Utc::now().timestamp_millis();

        // 尝试 Handler 模式
        let handler_opt = {
            let handlers = self.handlers.read().await;
            handlers.get(name).cloned()
        };

        if let Some(handler) = handler_opt {
            debug!(workflow = %name, "executing workflow via handler");
            let result = handler(name.to_string(), ctx, input.clone()).await;

            // 记录执行日志
            let mut log = self.execution_log.write().await;
            let records = log.entry(name.to_string()).or_default();
            match &result {
                Ok(output) => {
                    records.push(ExecutionRecord {
                        state: ExecutionState::Completed,
                        input: input.clone(),
                        output: Some(output.clone()),
                        started_at: now_ms,
                        completed_at: Some(chrono::Utc::now().timestamp_millis()),
                    });
                    info!(workflow = %name, "workflow execution completed");
                }
                Err(e) => {
                    records.push(ExecutionRecord {
                        state: ExecutionState::Failed(e.to_string()),
                        input: input.clone(),
                        output: None,
                        started_at: now_ms,
                        completed_at: Some(chrono::Utc::now().timestamp_millis()),
                    });
                    warn!(workflow = %name, error = %e, "workflow execution failed");
                }
            }

            return result;
        }

        // 模板模式：检查注册的定义
        let def = {
            let reg = self.registry.read().await;
            reg.get(name).cloned()
        };

        match def {
            Some(workflow_def) => {
                // 直接返回定义 + 输入作为执行结果
                let output = serde_json::json!({
                    "workflow": workflow_def.name,
                    "definition": workflow_def.definition,
                    "input": input,
                    "status": "template_mode",
                    "note": "No async handler registered; returned definition as result",
                });

                // 记录执行日志
                let mut log = self.execution_log.write().await;
                log.entry(name.to_string())
                    .or_default()
                    .push(ExecutionRecord {
                        state: ExecutionState::Completed,
                        input: input.clone(),
                        output: Some(output.clone()),
                        started_at: now_ms,
                        completed_at: Some(chrono::Utc::now().timestamp_millis()),
                    });

                info!(workflow = %name, "workflow executed in template mode");
                Ok(output)
            }
            None => Err(lingshu_core::LsError::NotFound(format!(
                "workflow '{}' not found",
                name
            ))),
        }
    }

    /// 查询工作流状态.
    async fn workflow_status(&self, name: &str) -> LsResult<Value> {
        let exists = {
            let reg = self.registry.read().await;
            reg.contains_key(name)
        };

        if !exists {
            let handlers = self.handlers.read().await;
            if !handlers.contains_key(name) {
                return Err(lingshu_core::LsError::NotFound(format!(
                    "workflow '{}' not found",
                    name
                )));
            }
        }

        let history = {
            let log = self.execution_log.read().await;
            log.get(name).cloned().unwrap_or_default()
        };

        let last_state = history.last().map(|r| match &r.state {
            ExecutionState::Completed => "completed",
            ExecutionState::Failed(_) => "failed",
            ExecutionState::Running => "running",
            ExecutionState::Pending => "pending",
        });

        let last_output = history.last().and_then(|r| r.output.clone());

        Ok(serde_json::json!({
            "workflow": name,
            "exists": true,
            "total_executions": history.len(),
            "last_state": last_state.unwrap_or("never_executed"),
            "last_output": last_output,
            "execution_count": history.len(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let access = RuntimeWorkflowAccess::new();
        access
            .register("test-flow", serde_json::json!({"steps": ["a", "b"]}))
            .await;

        let workflows = access.list_workflows().await;
        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0]["name"], "test-flow");
    }

    #[tokio::test]
    async fn test_execute_template_mode() {
        let access = RuntimeWorkflowAccess::new();
        access
            .register("echo", serde_json::json!({"type": "echo"}))
            .await;

        let result = access
            .execute_workflow("echo", test_ctx(), serde_json::json!({"hello": "world"}))
            .await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["workflow"], "echo");
        assert_eq!(val["status"], "template_mode");
    }

    #[tokio::test]
    async fn test_execute_handler_mode() {
        let access = RuntimeWorkflowAccess::new();
        let handler: WorkflowHandler = Arc::new(|name, _ctx, input| {
            Box::pin(async move {
                Ok(serde_json::json!({
                    "processed": true,
                    "workflow": name,
                    "input": input,
                }))
            })
        });
        access.register_handler("compute", handler).await;

        let result = access
            .execute_workflow("compute", test_ctx(), serde_json::json!({"x": 42}))
            .await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["processed"], true);
        assert_eq!(val["input"]["x"], 42);
    }

    #[tokio::test]
    async fn test_status_not_found() {
        let access = RuntimeWorkflowAccess::new();
        let result = access.workflow_status("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_exists() {
        let access = RuntimeWorkflowAccess::new();
        access.register("exists", serde_json::json!({})).await;
        assert!(access.exists("exists").await);
        assert!(!access.exists("missing").await);
    }

    #[tokio::test]
    async fn test_execution_history() {
        let access = RuntimeWorkflowAccess::new();
        access.register("hist", serde_json::json!({"v": 1})).await;

        let _ = access
            .execute_workflow("hist", test_ctx(), serde_json::json!({"n": 1}))
            .await;
        let _ = access
            .execute_workflow("hist", test_ctx(), serde_json::json!({"n": 2}))
            .await;

        let history = access.get_execution_history("hist").await;
        assert_eq!(history.len(), 2);
    }
}
