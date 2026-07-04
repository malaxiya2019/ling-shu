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

/// 插件清单 (plugin.json / manifest).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    pub permissions: Vec<PluginPermission>,
    /// 最低 API 版本
    pub min_api_version: Option<String>,
}

/// 插件元信息.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginInfo {
    pub plugin_id: LsId,
    pub manifest: PluginManifest,
    pub status: PluginStatus,
    pub loaded_at: Option<chrono::DateTime<chrono::Utc>>,
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
}
