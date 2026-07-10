//! Plugin — 插件接口与类型定义。

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};

/// 插件状态.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum PluginStatus {
    #[default]
    Installed,
    Loaded,
    Running,
    Stopped,
    Failed(String),
}

/// 插件权限声明.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPermission {
    pub resource: String,
    pub actions: Vec<String>,
}

/// 插件能力声明 — 描述插件提供的功能类别.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// 能力名称 (如 "search", "storage", "llm").
    pub name: String,
    /// 能力描述.
    pub description: Option<String>,
    /// 版本约束 (如 ">=1.0.0").
    pub version_req: Option<String>,
}

/// 工具声明 — 插件标记其提供的工具.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDeclaration {
    /// 工具名称 (用于 ToolRegistry 注册).
    pub name: String,
    /// 工具描述.
    pub description: String,
    /// 所需权限级别.
    #[serde(default)]
    pub permission_level: String,
}

/// 插件清单 (plugin.json / manifest).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    /// 插件类型: "static" | "dynamic"
    pub plugin_type: String,
    /// 入口符号 (动态插件: .so/.dylib 符号名)
    pub entry_point: Option<String>,
    /// 所需运行时权限
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    /// 最低 API 版本
    pub min_api_version: Option<String>,
    // ── v3.8+ 扩展字段 ──
    /// 插件提供的能力列表.
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// 插件声明的工具列表.
    #[serde(default)]
    pub tools: Vec<ToolDeclaration>,
}

impl Default for PluginManifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: "1.0.0".into(),
            description: String::new(),
            author: None,
            homepage: None,
            license: None,
            plugin_type: "static".into(),
            entry_point: None,
            permissions: Vec::new(),
            min_api_version: None,
            capabilities: Vec::new(),
            tools: Vec::new(),
        }
    }
}

/// 插件元信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub plugin_id: LsId,
    pub manifest: PluginManifest,
    pub status: PluginStatus,
    pub loaded_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for PluginInfo {
    fn default() -> Self {
        Self {
            plugin_id: LsId::new(),
            manifest: PluginManifest::default(),
            status: PluginStatus::Installed,
            loaded_at: None,
        }
    }
}

/// Plugin — 插件加载、卸载、权限声明、能力适配.
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// 返回插件元信息.
    fn info(&self) -> PluginInfo;

    /// 初始化插件.
    async fn init(&self, ctx: LsContext) -> LsResult<()>;

    /// 启动插件.
    async fn start(&self, ctx: LsContext) -> LsResult<()>;

    /// 停止插件.
    async fn stop(&self, ctx: LsContext) -> LsResult<()>;

    /// 声明所需权限.
    fn required_permissions(&self) -> Vec<PluginPermission>;

    /// 返回 ToolProvider 引用（如果插件实现了 ToolProvider）.
    /// 默认实现返回 None，实现 ToolProvider 的插件应覆盖此方法.
    fn as_tool_provider(&self) -> Option<&dyn ToolProvider> {
        None
    }
}

/// ToolProvider — 有能力提供工具的插件扩展接口.
///
/// 实现此 trait 的插件可以向 ToolRegistry 注册工具。
/// 在插件注册/初始化阶段自动调用。
#[async_trait]
pub trait ToolProvider: Plugin {
    /// 返回插件提供的工具列表.
    fn provided_tools(&self) -> Vec<Box<dyn crate::tool::Tool>>;
}
