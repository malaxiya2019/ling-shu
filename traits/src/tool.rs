//! Tool — 工具接口契约。
//!
//! 定义全系统统一的工具接口：`info()`、`validate()`、`execute()`。
//! 所有工具实现均通过此 trait 注册到 ToolRegistry 中。

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 工具权限级别.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, Default)]
pub enum PermissionLevel {
    /// 公开可用 — 无需特定权限
    #[default]
    Public,
    /// 需要用户级权限（已登录用户）
    User,
    /// 需要管理员权限
    Admin,
    /// 需要超级管理员权限
    SuperAdmin,
}

impl std::fmt::Display for PermissionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::User => write!(f, "user"),
            Self::Admin => write!(f, "admin"),
            Self::SuperAdmin => write!(f, "super_admin"),
        }
    }
}

/// 工具分类.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, Default)]
pub enum ToolCategory {
    /// 通用工具（默认）
    #[default]
    General,
    /// 文件系统操作（读/写/列表）
    FileSystem,
    /// 网络/HTTP 请求
    Network,
    /// Shell 命令执行
    Shell,
    /// 代码分析
    CodeAnalysis,
    /// 数据检索（RAG/搜索）
    Retrieval,
    /// AI/LLM 相关
    AI,
    /// 系统管理
    System,
    /// 通信/消息
    Communication,
    /// 自定义分类
    Custom(String),
}

impl ToolCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::General => "general",
            Self::FileSystem => "filesystem",
            Self::Network => "network",
            Self::Shell => "shell",
            Self::CodeAnalysis => "code_analysis",
            Self::Retrieval => "retrieval",
            Self::AI => "ai",
            Self::System => "system",
            Self::Communication => "communication",
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 工具参数定义.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolParam {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub param_type: String,
}

/// 沙箱配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// 最大执行时间（毫秒）.
    pub max_execution_ms: u64,
    /// 最大输出大小（字节）.
    pub max_output_bytes: u64,
    /// 是否需要网络隔离.
    pub network_isolated: bool,
    /// 是否需要文件系统隔离.
    pub fs_isolated: bool,
    /// 允许的内存上限（MB）.
    pub max_memory_mb: Option<u64>,
    /// 特殊权限（如 "sudo", "docker" 等）.
    pub special_permissions: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_execution_ms: 30_000,
            max_output_bytes: 1_000_000,
            network_isolated: false,
            fs_isolated: false,
            max_memory_mb: None,
            special_permissions: Vec::new(),
        }
    }
}

/// 工具元信息（扩展字段集合，v3.7+）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// 工具分类.
    pub category: ToolCategory,
    /// 搜索标签.
    pub tags: Vec<String>,
    /// 所需权限级别.
    pub permission_level: PermissionLevel,
    /// 自定义超时（毫秒），None 使用系统默认.
    pub timeout_ms: Option<u64>,
    /// 沙箱配置，None 表示无需沙箱.
    pub sandbox_config: Option<SandboxConfig>,
    /// 版本号.
    pub version: String,
    /// 作者/提供商.
    pub author: String,
}

impl Default for ToolMetadata {
    fn default() -> Self {
        Self {
            category: ToolCategory::General,
            tags: Vec::new(),
            permission_level: PermissionLevel::Public,
            timeout_ms: None,
            sandbox_config: None,
            version: "1.0.0".into(),
            author: "lingshu".into(),
        }
    }
}

/// 工具元信息.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolInfo {
    pub tool_id: LsId,
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParam>,

    // ── 扩展字段 (v3.7+) ──
    /// 工具元信息（分类/标签/权限/沙箱等）.
    pub metadata: ToolMetadata,
}

impl ToolInfo {
    /// 创建基础 ToolInfo（保留向后兼容）.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Vec<ToolParam>,
    ) -> Self {
        Self {
            tool_id: LsId::new(),
            name: name.into(),
            description: description.into(),
            parameters,
            metadata: ToolMetadata::default(),
        }
    }

    /// 设置分类.
    pub fn with_category(mut self, category: ToolCategory) -> Self {
        self.metadata.category = category;
        self
    }

    /// 添加标签.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.metadata.tags = tags;
        self
    }

    /// 设置权限级别.
    pub fn with_permission(mut self, level: PermissionLevel) -> Self {
        self.metadata.permission_level = level;
        self
    }

    /// 设置超时.
    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.metadata.timeout_ms = Some(ms);
        self
    }

    /// 设置沙箱配置.
    pub fn with_sandbox(mut self, config: SandboxConfig) -> Self {
        self.metadata.sandbox_config = Some(config);
        self
    }
}

/// 工具调用记录.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolCallRecord {
    pub tool_id: LsId,
    pub call_id: LsId,
    pub session_id: LsId,
    pub input: Value,
    pub output: Value,
    pub duration_ms: u64,
    pub success: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    // ── 扩展字段 (v3.7+) ──
    /// 调用的用户/角色.
    pub caller: Option<String>,
    /// 错误信息（失败时）.
    pub error: Option<String>,
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

    /// 克隆工具实例（用于缓存返回）.
    fn duplicate(&self) -> Box<dyn Tool>;
}
