//! 📱 agent-device plugin — 移动设备自动化
//!
//! 将 [agent-device](https://github.com/callstack/agent-device) 设备自动化 CLI
//! 集成到 Lingshu 中，让 AI Agent 可以操控 iOS、Android、TV、桌面应用。
//!
//! ## 功能
//!
//! - 自动发现 agent-device 的 MCP 工具（70+ 个命令）
//! - 子进程生命周期管理（启动/停止/健康检查/自动重启）
//! - 工具自动注册到 Lingshu ToolRegistry
//! - 会话管理器：跟踪多设备多应用生命周期
//! - 动态工具同步：后台定期检测工具列表变化
//! - 支持 iOS 模拟器、Android 模拟器、真机、桌面应用
//! - 平台过滤（iOS / Android / Linux / all）
//!
//! ## 前置条件
//!
//! - Node.js >= 22.12
//! - 已安装 `agent-device` npm 包: `npm install -g agent-device`
//! - 目标平台工具链（Xcode / Android SDK 等）
//!
//! ## 用法
//!
//! ```ignore
//! use lingshu_agent_device_plugin::{init_agent_device, AgentDeviceConfig};
//!
//! let plugin = init_agent_device(&tool_registry, AgentDeviceConfig::default()).await?;
//! ```

mod tool_bridge;
mod session_manager;
mod sync_task;

pub use session_manager::{SessionManager, SessionInfo, SessionPlatform, SessionStats};
pub use sync_task::{McpToolSyncTask, SyncStatus, ToolSyncDiff};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_mcp::rmcp_stdio_client::{McpStdioClient, McpStdioConfig, McpTool};
use lingshu_traits::plugin::{
    Capability, Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus, ToolDeclaration,
    ToolProvider,
};
use lingshu_traits::tool::Tool;
use tokio::sync::Mutex;
use tool_bridge::McpBridgeTool;

/// agent-device 插件配置.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentDeviceConfig {
    /// agent-device 命令路径 (默认 "agent-device").
    pub command: String,
    /// 额外参数.
    pub args: Vec<String>,
    /// 工作目录.
    pub work_dir: Option<String>,
    /// 工具调用超时（毫秒），默认 120s.
    pub tool_timeout_ms: u64,
    /// 最大重启次数.
    pub max_restarts: u32,
    /// 平台过滤 (可选：ios / android / linux / all).
    pub platform: Option<String>,
    /// 标签.
    pub tags: Vec<String>,
    /// 输出格式 (optimized / json).
    pub output_format: String,
    /// 自动同步间隔（秒），0 表示禁用自动同步.
    pub auto_sync_interval_secs: u64,
    /// 最大空闲会话时间（秒），0 表示不禁用.
    pub max_session_idle_secs: u64,
}

impl Default for AgentDeviceConfig {
    fn default() -> Self {
        Self {
            command: "agent-device".into(),
            args: vec!["mcp".into()],
            work_dir: None,
            tool_timeout_ms: 120_000,
            max_restarts: 3,
            platform: None,
            tags: vec![
                "device".into(),
                "mobile".into(),
                "automation".into(),
                "testing".into(),
            ],
            output_format: "optimized".into(),
            auto_sync_interval_secs: 30,
            max_session_idle_secs: 3600, // 1 小时
        }
    }
}

/// agent-device 插件（v2 — 增强版）.
pub struct AgentDevicePlugin {
    info: PluginInfo,
    config: AgentDeviceConfig,
    client: Mutex<Option<Arc<McpStdioClient>>>,
    running: AtomicBool,
    discovered_tools: Arc<tokio::sync::RwLock<Vec<Box<dyn Tool>>>>,
    /// 会话管理器（使用内部可变性）
    session_manager: tokio::sync::Mutex<Option<Arc<SessionManager>>>,
    /// 同步任务（使用内部可变性）
    sync_task: tokio::sync::Mutex<Option<Arc<McpToolSyncTask>>>,
}

impl AgentDevicePlugin {
    /// 使用默认配置创建插件.
    pub fn new(command: String) -> Self {
        let config = AgentDeviceConfig {
            command,
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// 使用自定义配置创建插件.
    pub fn with_config(config: AgentDeviceConfig) -> Self {
        let manifest = PluginManifest {
            name: "agent-device-plugin".into(),
            version: "2.0.0".into(),
            description: "agent-device — 移动设备自动化 AI Agent 工具 (iOS/Android/TV/桌面)".into(),
            author: Some("Callstack".into()),
            homepage: Some("https://agent-device.dev".into()),
            license: Some("MIT".into()),
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![
                PluginPermission {
                    resource: "process".into(),
                    actions: vec!["spawn".into(), "kill".into()],
                },
                PluginPermission {
                    resource: "network".into(),
                    actions: vec!["tcp".into()],
                },
                PluginPermission {
                    resource: "device".into(),
                    actions: vec![
                        "open".into(), "tap".into(), "type".into(),
                        "scroll".into(), "screenshot".into(),
                    ],
                },
            ],
            min_api_version: Some("5.0.0".into()),
            capabilities: vec![
                Capability {
                    name: "device-automation".into(),
                    description: Some("iOS/Android/TV/Desktop 设备自动化能力".into()),
                    version_req: None,
                },
                Capability {
                    name: "mobile-testing".into(),
                    description: Some("移动应用 UI 测试能力".into()),
                    version_req: None,
                },
                Capability {
                    name: "session-management".into(),
                    description: Some("多设备多会话生命周期管理".into()),
                    version_req: None,
                },
                Capability {
                    name: "tool-sync".into(),
                    description: Some("动态 MCP 工具列表自动同步".into()),
                    version_req: None,
                },
            ],
            tools: vec![
                ToolDeclaration {
                    name: "device:*".into(),
                    description: "agent-device 设备自动化工具集".into(),
                    permission_level: "admin".into(),
                },
            ],
        };

        let info = PluginInfo {
            plugin_id: LsId::new(),
            manifest,
            status: PluginStatus::Installed,
            loaded_at: None,
        };

        Self {
            info,
            config,
            client: Mutex::new(None),
            running: AtomicBool::new(false),
            discovered_tools: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            session_manager: tokio::sync::Mutex::new(None),
            sync_task: tokio::sync::Mutex::new(None),
        }
    }

    /// 获取插件运行状态.
    pub async fn plugin_status(&self) -> serde_json::Value {
        let mcp_running = self.running.load(Ordering::SeqCst);
        let tool_count = self.discovered_tools.read().await.len();

        let session_count = {
            let guard = self.session_manager.lock().await;
            match guard.as_ref() {
                Some(sm) => Some(sm.stats().await.total_sessions),
                None => None,
            }
        };

        serde_json::json!({
            "plugin_id": self.info.plugin_id.to_string(),
            "name": self.info.manifest.name,
            "version": "2.0.0",
            "status": format!("{:?}", self.info.status),
            "running": mcp_running,
            "discovered_tools": tool_count,
            "active_sessions": session_count,
            "sync_enabled": self.config.auto_sync_interval_secs > 0,
            "config": {
                "command": self.config.command,
                "args": self.config.args,
                "tool_timeout_ms": self.config.tool_timeout_ms,
                "platform": self.config.platform,
                "output_format": self.config.output_format,
                "auto_sync_interval_secs": self.config.auto_sync_interval_secs,
            }
        })
    }

    /// 获取会话管理器引用.
    pub async fn session_manager(&self) -> Option<Arc<SessionManager>> {
        self.session_manager.lock().await.clone()
    }

    /// 获取同步任务引用.
    pub async fn sync_task(&self) -> Option<Arc<McpToolSyncTask>> {
        self.sync_task.lock().await.clone()
    }

    /// 获取已发现的工具（需在 start() 之后调用）.
    pub async fn discovered_tools(&self) -> Vec<Box<dyn Tool>> {
        let tools = self.discovered_tools.read().await;
        tools.iter().map(|t| t.duplicate()).collect()
    }

    /// 检查系统依赖是否满足.
    pub async fn check_dependencies() -> AgentDeviceDeps {
        let node_ok = check_command("node", &["--version"]).await;
        let agent_device_ok = check_command("agent-device", &["--version"]).await;
        let xcode_ok = check_command("xcode-select", &["-p"]).await;
        let adb_ok = check_command("adb", &["version"]).await;

        AgentDeviceDeps {
            node_installed: node_ok,
            agent_device_installed: agent_device_ok,
            xcode_installed: xcode_ok,
            adb_installed: adb_ok,
        }
    }

    /// 根据平台名称过滤支持的 MCP 工具.
    fn filter_tools_by_platform<'a>(&self, tools: &'a [McpTool], platform: &str) -> Vec<&'a McpTool> {
        match platform {
            "ios" | "android" => tools.iter().collect(),
            "linux" => tools.iter().filter(|t| self.supports_linux(&t.name)).collect(),
            _ => tools.iter().collect(),
        }
    }

    /// 检查工具是否支持 Linux 桌面.
    fn supports_linux(&self, name: &str) -> bool {
        !matches!(name,
            "apps" | "boot" | "shutdown" | "perf" | "logs" | "network"
            | "audio" | "install" | "reinstall" | "push" | "trigger-app-event"
            | "keyboard" | "metro" | "session" | "app-switcher" | "install-from-source"
            | "rotate" | "tv-remote" | "viewport" | "record" | "trace"
            | "react-native"
        )
    }

    /// 启动 MCP 子进程并发现工具.
    pub async fn start_and_discover(&self, ctx: &LsContext) -> LsResult<Vec<Box<dyn Tool>>> {
        let deps = Self::check_dependencies().await;
        if !deps.all_ok() {
            return Err(LsError::Plugin(format!(
                "agent-device dependencies not satisfied: {}",
                deps.summary()
            )));
        }
        tracing::info!("agent-device deps: {}", deps.summary());

        let mcp_config = McpStdioConfig {
            command: self.config.command.clone(),
            args: self.config.args.clone(),
            work_dir: self.config.work_dir.clone(),
            tool_timeout_ms: self.config.tool_timeout_ms,
            max_restarts: self.config.max_restarts,
            ..Default::default()
        };

        let client = McpStdioClient::spawn(mcp_config).await?;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        client.health_check().await.map_err(|e| {
            LsError::Plugin(format!("agent-device MCP health check failed: {e}"))
        })?;

        let tools = client.list_tools().await?;
        tracing::info!(
            plugin = "agent-device-plugin",
            tool_count = tools.len(),
            "Discovered agent-device MCP tools"
        );

        let client_arc = Arc::new(client);
        let mut bridge_tools: Vec<Box<dyn Tool>> = Vec::new();

        let filtered_tools: Vec<&McpTool> = if let Some(ref platform) = self.config.platform {
            let platform_lower = platform.to_lowercase();
            let supported = self.filter_tools_by_platform(&tools, &platform_lower);
            tracing::info!(
                plugin = "agent-device-plugin",
                platform = %platform_lower,
                total_tools = tools.len(),
                filtered_tools = supported.len(),
                "Platform filter applied"
            );
            supported
        } else {
            tools.iter().collect()
        };

        for tool in filtered_tools {
            let bridge = McpBridgeTool::new(
                tool.name.clone(),
                tool.description.clone(),
                tool.input_schema.clone(),
                client_arc.clone(),
                self.config.output_format.clone(),
            );
            bridge_tools.push(Box::new(bridge));
        }

        *self.discovered_tools.write().await = bridge_tools.iter()
            .map(|t| t.duplicate())
            .collect();
        *self.client.lock().await = Some(client_arc.clone());
        self.running.store(true, Ordering::SeqCst);

        // 初始化会话管理器
        let session_manager = Arc::new(SessionManager::new());
        *self.session_manager.lock().await = Some(session_manager);

        // 自动同步（如果启用）
        if self.config.auto_sync_interval_secs > 0 {
            let registry = lingshu_tool::ToolRegistry::new();
            let sync_task = Arc::new(McpToolSyncTask::new(
                client_arc.clone(),
                registry,
                ctx.clone(),
            ));
            sync_task.clone().start(Duration::from_secs(self.config.auto_sync_interval_secs));
            *self.sync_task.lock().await = Some(sync_task);

            tracing::info!(
                plugin = "agent-device-plugin",
                interval_secs = self.config.auto_sync_interval_secs,
                "Auto sync enabled"
            );
        }

        tracing::info!(
            plugin = "agent-device-plugin",
            tool_count = bridge_tools.len(),
            "agent-device MCP server started (v2)"
        );

        Ok(bridge_tools)
    }

    /// 手动触发一次工具同步（同步任务启用时有效）
    pub async fn sync_now(&self) -> LsResult<ToolSyncDiff> {
        let guard = self.sync_task.lock().await;
        match guard.as_ref() {
            Some(task) => task.sync_and_diff().await,
            None => Err(LsError::Plugin("Auto sync is not enabled".into())),
        }
    }
}

/// 系统依赖检查结果.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentDeviceDeps {
    pub node_installed: bool,
    pub agent_device_installed: bool,
    pub xcode_installed: bool,
    pub adb_installed: bool,
}

impl AgentDeviceDeps {
    pub fn all_ok(&self) -> bool {
        self.node_installed && self.agent_device_installed
    }

    pub fn summary(&self) -> String {
        format!(
            "Node.js: {}, agent-device: {}, Xcode: {}, ADB: {}",
            if self.node_installed { "✅" } else { "❌" },
            if self.agent_device_installed { "✅" } else { "❌" },
            if self.xcode_installed { "✅" } else { "❌" },
            if self.adb_installed { "✅" } else { "❌" },
        )
    }
}

async fn check_command(cmd: &str, args: &[&str]) -> bool {
    tokio::process::Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[async_trait]
impl Plugin for AgentDevicePlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(
            plugin = "agent-device-plugin",
            command = %self.config.command,
            version = "2.0.0",
            "agent-device plugin initialized"
        );
        Ok(())
    }

    async fn start(&self, ctx: LsContext) -> LsResult<()> {
        self.start_and_discover(&ctx).await?;
        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "agent-device-plugin", "Stopping agent-device (v2)...");
        self.running.store(false, Ordering::SeqCst);

        // 停止同步任务
        let mut sync_guard = self.sync_task.lock().await;
        if let Some(sync_task) = sync_guard.take() {
            sync_task.stop();
        }
        drop(sync_guard);

        // 清理会话
        let mut sess_guard = self.session_manager.lock().await;
        if let Some(session_mgr) = sess_guard.take() {
            let _ = session_mgr.clean_stale_sessions(0).await;
        }
        drop(sess_guard);

        // 优雅关闭 MCP 子进程
        let mut client_guard = self.client.lock().await;
        if let Some(client) = client_guard.take() {
            if let Err(e) = client.shutdown().await {
                tracing::warn!(plugin = "agent-device-plugin", error = %e, "MCP shutdown warning");
            }
        }
        drop(client_guard);

        self.discovered_tools.write().await.clear();
        tracing::info!(plugin = "agent-device-plugin", "agent-device stopped (v2)");
        Ok(())
    }

    fn required_permissions(&self) -> Vec<PluginPermission> {
        self.info.manifest.permissions.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_tool_provider(&self) -> Option<&dyn ToolProvider> {
        Some(self)
    }
}

impl ToolProvider for AgentDevicePlugin {
    fn provided_tools(&self) -> Vec<Box<dyn Tool>> {
        Vec::new()
    }
}

// ── 一站式初始化 ──────────────────────────────────

/// 一站式初始化 agent-device：启动 MCP 子进程、发现并注册工具到 ToolRegistry.
///
/// 这是推荐的使用方式 — 一次调用完成所有设置。
///
/// ## 示例
///
/// ```ignore
/// use lingshu_agent_device_plugin::{init_agent_device, AgentDeviceConfig};
///
/// let plugin = init_agent_device(&tool_registry, AgentDeviceConfig::default()).await?;
///
/// // 自定义配置
/// let config = AgentDeviceConfig {
///     command: "/usr/local/bin/agent-device".into(),
///     tool_timeout_ms: 60_000,
///     output_format: "json".into(),
///     auto_sync_interval_secs: 60,
///     ..Default::default()
/// };
/// let plugin = init_agent_device(&tool_registry, config).await?;
/// ```
pub async fn init_agent_device(
    tool_registry: &lingshu_tool::ToolRegistry,
    config: AgentDeviceConfig,
) -> LsResult<AgentDevicePlugin> {
    let plugin = AgentDevicePlugin::with_config(config);
    let ctx = LsContext::with_session(LsId::new());

    tracing::info!("agent-device: starting MCP subprocess and discovering tools (v2)...");

    let tools = plugin.start_and_discover(&ctx).await?;

    let count = tools.len();
    for tool in tools {
        tool_registry.register(tool).await;
    }

    tracing::info!("agent-device: registered {count} tools to ToolRegistry");
    Ok(plugin)
}

/// 动态加载入口.
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    Box::new(AgentDevicePlugin::new("agent-device".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_info() {
        let plugin = AgentDevicePlugin::new("agent-device".into());
        let info = plugin.info();
        assert_eq!(info.manifest.name, "agent-device-plugin");
        assert_eq!(info.manifest.plugin_type, "static");
    }

    #[test]
    fn test_plugin_capabilities() {
        let plugin = AgentDevicePlugin::new("agent-device".into());
        let info = plugin.info();
        assert!(info.manifest.capabilities.iter().any(|c| c.name == "device-automation"));
        assert!(info.manifest.capabilities.iter().any(|c| c.name == "mobile-testing"));
        assert!(info.manifest.capabilities.iter().any(|c| c.name == "session-management"));
        assert!(info.manifest.capabilities.iter().any(|c| c.name == "tool-sync"));
    }

    #[test]
    fn test_as_any_downcast() {
        use std::any::Any;
        use lingshu_traits::plugin::Plugin;

        let plugin = AgentDevicePlugin::new("agent-device".into());
        let any_ref: &dyn Any = Plugin::as_any(&plugin);
        let downcasted = any_ref.downcast_ref::<AgentDevicePlugin>();
        assert!(downcasted.is_some(), "Should downcast back to AgentDevicePlugin");
        let info = downcasted.unwrap().info();
        assert_eq!(info.manifest.name, "agent-device-plugin");
    }

    #[test]
    fn test_config_platform_filter() {
        let config = AgentDeviceConfig {
            platform: Some("ios".into()),
            ..Default::default()
        };
        assert_eq!(config.platform.as_deref(), Some("ios"));

        let config_all = AgentDeviceConfig {
            platform: Some("all".into()),
            ..Default::default()
        };
        assert_eq!(config_all.platform.as_deref(), Some("all"));

        let default = AgentDeviceConfig::default();
        assert!(default.platform.is_none());
    }

    #[test]
    fn test_plugin_permissions() {
        let plugin = AgentDevicePlugin::new("agent-device".into());
        let perms = plugin.required_permissions();
        assert!(perms.iter().any(|p| p.resource == "process"));
        assert!(perms.iter().any(|p| p.resource == "device"));
    }

    #[test]
    fn test_config_default() {
        let config = AgentDeviceConfig::default();
        assert_eq!(config.command, "agent-device");
        assert_eq!(config.tool_timeout_ms, 120_000);
        assert_eq!(config.output_format, "optimized");
        assert_eq!(config.auto_sync_interval_secs, 30);
    }

    #[test]
    fn test_deps() {
        let deps = AgentDeviceDeps {
            node_installed: true,
            agent_device_installed: true,
            xcode_installed: false,
            adb_installed: true,
        };
        assert!(deps.all_ok());
        assert!(deps.summary().contains("✅"));
    }

    #[test]
    fn test_supports_linux() {
        let plugin = AgentDevicePlugin::new("agent-device".into());
        assert!(plugin.supports_linux("snapshot"));
        assert!(plugin.supports_linux("open"));
        assert!(plugin.supports_linux("click"));
        assert!(!plugin.supports_linux("apps"));
        assert!(!plugin.supports_linux("boot"));
        assert!(!plugin.supports_linux("keyboard"));
    }

    #[test]
    fn test_filter_tools_by_platform_ios() {
        let plugin = AgentDevicePlugin::new("agent-device".into());
        let all_tools = vec![
            McpTool { name: "snapshot".into(), description: "s".into(), input_schema: serde_json::Value::Null },
            McpTool { name: "keyboard".into(), description: "k".into(), input_schema: serde_json::Value::Null },
            McpTool { name: "apps".into(), description: "a".into(), input_schema: serde_json::Value::Null },
        ];
        let filtered = plugin.filter_tools_by_platform(&all_tools, "ios");
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_filter_tools_by_platform_linux() {
        let plugin = AgentDevicePlugin::new("agent-device".into());
        let all_tools = vec![
            McpTool { name: "snapshot".into(), description: "snapshot".into(), input_schema: serde_json::Value::Null },
            McpTool { name: "apps".into(), description: "apps".into(), input_schema: serde_json::Value::Null },
        ];
        let filtered = plugin.filter_tools_by_platform(&all_tools, "linux");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "snapshot");
    }

    #[test]
    fn test_plugin_version() {
        let plugin = AgentDevicePlugin::new("agent-device".into());
        assert_eq!(plugin.info.manifest.version, "2.0.0");
    }
}
