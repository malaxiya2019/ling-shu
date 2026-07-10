//! Tool Runtime 扩展类型.
//!
//! 提供工具注册、查询、过滤所需的额外类型.

use serde::{Deserialize, Serialize};

/// 工具查询过滤器.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFilter {
    /// 按分类过滤.
    pub category: Option<String>,
    /// 按标签过滤（任一匹配即可）.
    pub tags: Option<Vec<String>>,
    /// 按名称搜索（模糊匹配）.
    pub name_search: Option<String>,
    /// 按最低权限级别过滤.
    pub min_permission: Option<String>,
    /// 是否只返回启用的工具.
    pub only_enabled: bool,
}

impl Default for ToolFilter {
    fn default() -> Self {
        Self {
            category: None,
            tags: None,
            name_search: None,
            min_permission: None,
            only_enabled: true,
        }
    }
}

/// 执行结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    /// 工具调用 ID.
    pub call_id: String,
    /// 工具名称.
    pub tool_name: String,
    /// 执行是否成功.
    pub success: bool,
    /// 执行输出.
    pub output: serde_json::Value,
    /// 执行耗时（毫秒）.
    pub duration_ms: u64,
    /// 错误信息（如失败）.
    pub error: Option<String>,
    /// 时间戳.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 工具统计信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    /// 工具总数.
    pub total_count: usize,
    /// 按分类统计.
    pub by_category: std::collections::HashMap<String, usize>,
    /// 总调用次数.
    pub total_calls: u64,
    /// 最近调用.
    pub recent_calls: Vec<ToolExecutionResult>,
}

/// 沙箱执行限制.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxLimits {
    /// 最大执行时间（毫秒）.
    pub max_execution_ms: u64,
    /// 最大输出字节.
    pub max_output_bytes: u64,
    /// 最大内存（MB）.
    pub max_memory_mb: Option<u64>,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            max_execution_ms: 30_000,
            max_output_bytes: 1_000_000,
            max_memory_mb: None,
        }
    }
}
