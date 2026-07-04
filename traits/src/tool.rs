use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 工具参数定义.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub param_type: String,
}

/// 工具元信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub tool_id: LsId,
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParam>,
}

/// 工具调用记录.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub tool_id: LsId,
    pub call_id: LsId,
    pub session_id: LsId,
    pub input: Value,
    pub output: Value,
    pub duration_ms: u64,
    pub success: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Tool — 工具定义、参数校验、执行与审计.
#[async_trait]
pub trait Tool: Send + Sync + 'static {
    /// 返回工具元信息.
    fn info(&self) -> ToolInfo;

    /// 校验参数.
    fn validate(&self, input: &Value) -> LsResult<()>;

    /// 执行工具调用.
    async fn execute(&self, ctx: LsContext, input: Value) -> LsResult<Value>;
}
