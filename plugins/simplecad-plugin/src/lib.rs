//! 🖨️ SimpleCAD 插件 — 3D CAD 建模
//!
//! 将 [SimpleCADAPI](https://github.com/NiJingzhe/SimpleCADAPI) Python CAD 建模 SDK
//! 通过 MCP Stdio 协议集成到 Lingshu 中。
//!
//! ## 前置条件
//!
//! - Python >= 3.10 + `pip install simplecadapi mcp`
//!
//! ## 用法
//!
//! ```ignore
//! use lingshu_simplecad_plugin::{init_simplecad, SimpleCadConfig};
//!
//! let plugin = init_simplecad(&tool_registry, SimpleCadConfig::default()).await?;
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult, LsId};
use lingshu_mcp::rmcp_stdio_client::{McpStdioClient, McpStdioConfig, McpToolResult, McpContent};
use lingshu_traits::plugin::{
    Capability, Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus, ToolDeclaration,
    ToolProvider,
};
use lingshu_traits::tool::{
    Tool, ToolInfo, ToolMetadata, ToolParam, ToolCategory, PermissionLevel, SandboxConfig,
};
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{info, warn};

// ── 配置 ─────────────────────────────────────────────

/// SimpleCAD 插件配置.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimpleCadConfig {
    /// Python 解释器路径 (默认 "python3").
    pub python_cmd: String,
    /// MCP 服务器模块路径.
    pub server_module: String,
    /// 工作目录.
    pub work_dir: Option<String>,
    /// 工具调用超时（毫秒），默认 120s.
    pub tool_timeout_ms: u64,
    /// 最大重启次数.
    pub max_restarts: u32,
}

impl Default for SimpleCadConfig {
    fn default() -> Self {
        Self {
            python_cmd: "python3".into(),
            server_module: "simplecad_mcp.server".into(),
            work_dir: None,
            tool_timeout_ms: 120_000,
            max_restarts: 2,
        }
    }
}

// ── 插件结构体 ───────────────────────────────────────

/// SimpleCAD 建模插件.
pub struct SimpleCadPlugin {
    info: PluginInfo,
    config: SimpleCadConfig,
    client: Mutex<Option<Arc<McpStdioClient>>>,
    running: AtomicBool,
    discovered_tools: Arc<tokio::sync::RwLock<Vec<Box<dyn Tool>>>>,
}

impl SimpleCadPlugin {
    /// 使用默认配置创建插件.
    pub fn new() -> Self {
        Self::with_config(SimpleCadConfig::default())
    }

    /// 使用自定义配置创建插件.
    pub fn with_config(config: SimpleCadConfig) -> Self {
        let manifest = PluginManifest {
            name: "simplecad-plugin".into(),
            version: "0.1.0".into(),
            description: "SimpleCAD — AI 驱动的 3D CAD 建模插件 (基于 SimpleCADAPI)".into(),
            author: Some("NiJingzhe / Lingshu".into()),
            homepage: Some("https://github.com/NiJingzhe/SimpleCADAPI".into()),
            license: Some("AGPL-3.0".into()),
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![
                PluginPermission {
                    resource: "process".into(),
                    actions: vec!["spawn".into()],
                },
                PluginPermission {
                    resource: "filesystem".into(),
                    actions: vec!["write".into()],
                },
            ],
            min_api_version: Some("5.0.0".into()),
            capabilities: vec![
                Capability {
                    name: "cad-modeling".into(),
                    description: Some("3D CAD 建模能力：基本体、布尔运算、特征、变换".into()),
                    version_req: None,
                },
                Capability {
                    name: "cad-export".into(),
                    description: Some("STEP/STL 文件导出".into()),
                    version_req: None,
                },
                Capability {
                    name: "cad-replay".into(),
                    description: Some("GraphSession 可重放建模".into()),
                    version_req: None,
                },
            ],
            tools: vec![
                ToolDeclaration {
                    name: "cad:*".into(),
                    description: "SimpleCAD 3D 建模工具集".into(),
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
        }
    }

    /// 启动 MCP 子进程并发现 CAD 工具.
    pub async fn start_and_discover(&self) -> LsResult<Vec<Box<dyn Tool>>> {
        let mcp_config = McpStdioConfig {
            command: self.config.python_cmd.clone(),
            args: vec!["-m".into(), self.config.server_module.clone()],
            work_dir: self.config.work_dir.clone(),
            tool_timeout_ms: self.config.tool_timeout_ms,
            max_restarts: self.config.max_restarts,
            ..Default::default()
        };

        info!(
            plugin = "simplecad-plugin",
            command = %mcp_config.command,
            args = ?mcp_config.args,
            "Starting SimpleCAD MCP server"
        );

        let client = McpStdioClient::spawn(mcp_config).await.map_err(|e| {
            LsError::Plugin(format!("SimpleCAD MCP 启动失败: {e}"))
        })?;

        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

        client.health_check().await.map_err(|e| {
            LsError::Plugin(format!("SimpleCAD MCP 健康检查失败: {e}"))
        })?;

        let tools = client.list_tools().await.map_err(|e| {
            LsError::Plugin(format!("SimpleCAD 工具列表获取失败: {e}"))
        })?;

        info!(
            plugin = "simplecad-plugin",
            tool_count = tools.len(),
            "Discovered SimpleCAD MCP tools"
        );

        let client_arc = Arc::new(client);
        let mut bridge_tools: Vec<Box<dyn Tool>> = Vec::new();

        for tool in &tools {
            let bridge = McpBridgeCadTool::new(
                tool.name.clone(),
                tool.description.clone(),
                tool.input_schema.clone(),
                client_arc.clone(),
            );
            bridge_tools.push(Box::new(bridge));
        }

        *self.discovered_tools.write().await = bridge_tools.iter()
            .map(|t| t.duplicate())
            .collect();
        *self.client.lock().await = Some(client_arc);
        self.running.store(true, Ordering::SeqCst);

        info!(
            plugin = "simplecad-plugin",
            tool_count = bridge_tools.len(),
            "SimpleCAD MCP server started"
        );

        Ok(bridge_tools)
    }

    /// 获取插件运行状态.
    pub async fn plugin_status(&self) -> Value {
        let running = self.running.load(Ordering::SeqCst);
        let tool_count = self.discovered_tools.read().await.len();

        serde_json::json!({
            "plugin_id": self.info.plugin_id.to_string(),
            "name": self.info.manifest.name,
            "version": "0.1.0",
            "status": format!("{:?}", self.info.status),
            "running": running,
            "discovered_tools": tool_count,
        })
    }

    /// 获取已发现的工具.
    pub async fn discovered_tools(&self) -> Vec<Box<dyn Tool>> {
        let tools = self.discovered_tools.read().await;
        tools.iter().map(|t| t.duplicate()).collect()
    }
}

impl Default for SimpleCadPlugin {
    fn default() -> Self {
        Self::new()
    }
}

// ── Plugin trait 实现 ────────────────────────────────

#[async_trait]
impl Plugin for SimpleCadPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        info!(plugin = "simplecad-plugin", version = "0.1.0", "SimpleCAD plugin initialized");
        Ok(())
    }

    async fn start(&self, _ctx: LsContext) -> LsResult<()> {
        self.start_and_discover().await?;
        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        info!(plugin = "simplecad-plugin", "Stopping SimpleCAD...");
        self.running.store(false, Ordering::SeqCst);

        let mut guard = self.client.lock().await;
        if let Some(client) = guard.take() {
            if let Err(e) = client.shutdown().await {
                warn!(plugin = "simplecad-plugin", error = %e, "Shutdown warning");
            }
        }
        drop(guard);

        self.discovered_tools.write().await.clear();
        info!(plugin = "simplecad-plugin", "SimpleCAD stopped");
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

impl ToolProvider for SimpleCadPlugin {
    fn provided_tools(&self) -> Vec<Box<dyn Tool>> {
        Vec::new()
    }
}

// ── 一站式初始化 ──────────────────────────────────────

/// 一站式初始化 SimpleCAD：启动 MCP 子进程、发现并注册工具到 ToolRegistry.
///
/// ```ignore
/// use lingshu_simplecad_plugin::{init_simplecad, SimpleCadConfig};
///
/// let plugin = init_simplecad(&tool_registry, SimpleCadConfig::default()).await?;
/// ```
pub async fn init_simplecad(
    tool_registry: &lingshu_tool::ToolRegistry,
    config: SimpleCadConfig,
) -> LsResult<SimpleCadPlugin> {
    let plugin = SimpleCadPlugin::with_config(config);
    info!("simplecad: starting MCP subprocess and discovering tools...");

    let tools = plugin.start_and_discover().await?;

    let count = tools.len();
    for tool in tools {
        tool_registry.register(tool).await;
    }

    info!("simplecad: registered {count} CAD tools to ToolRegistry");
    Ok(plugin)
}

// ── 工具桥接 ──────────────────────────────────────────

/// 将 MCP 工具包装为 Lingshu Tool.
struct McpBridgeCadTool {
    info: ToolInfo,
    client: Arc<McpStdioClient>,
}

impl McpBridgeCadTool {
    fn new(
        name: String,
        description: String,
        input_schema: Value,
        client: Arc<McpStdioClient>,
    ) -> Self {
        let params = extract_params(&input_schema);

        let info = ToolInfo {
            tool_id: LsId::new(),
            name,
            description,
            parameters: params,
            metadata: ToolMetadata {
                category: ToolCategory::Custom("cad".into()),
                tags: vec!["cad".into(), "modeling".into(), "3d".into()],
                permission_level: PermissionLevel::Admin,
                timeout_ms: Some(120_000),
                sandbox_config: Some(SandboxConfig {
                    max_execution_ms: 120_000,
                    max_output_bytes: 10_000_000,
                    network_isolated: true,
                    fs_isolated: false,
                    max_memory_mb: Some(1024),
                    special_permissions: vec![],
                }),
                version: "0.1.0".into(),
                author: "simplecad".into(),
            },
        };

        Self { info, client }
    }
}

#[async_trait]
impl Tool for McpBridgeCadTool {
    fn info(&self) -> ToolInfo {
        self.info.clone()
    }

    fn validate(&self, _input: &Value) -> LsResult<()> {
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        self.client
            .call_tool(&self.info.name, input)
            .await
            .map(|result| mcp_result_to_value(result))
            .map_err(|e| LsError::Plugin(format!("SimpleCAD MCP 调用失败: {e}")))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            info: self.info.clone(),
            client: self.client.clone(),
        })
    }
}

/// 从 JSON Schema 提取参数列表.
fn extract_params(schema: &Value) -> Vec<ToolParam> {
    let mut params = Vec::new();
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        let required_set: std::collections::HashSet<&str> = schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect()
            })
            .unwrap_or_default();

        for (name, prop) in properties {
            let desc = prop
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let param_type = prop
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("string")
                .to_string();
            let required = required_set.contains(name.as_str());

            params.push(ToolParam {
                name: name.clone(),
                description: desc,
                required,
                param_type,
            });
        }
    }
    params
}

/// 将 MCP 结果转换为 JSON Value.
fn mcp_result_to_value(result: McpToolResult) -> Value {
    let mut texts = Vec::new();
    for content in &result.content {
        match content {
            McpContent::Text { text } => texts.push(text.clone()),
            McpContent::Image { data, mime_type } => {
                texts.push(format!("[image: {} ({} bytes)]", mime_type, data.len()));
            }
            McpContent::Resource { uri, text, .. } => {
                if let Some(t) = text {
                    texts.push(format!("[resource: {uri}]\n{t}"));
                } else {
                    texts.push(format!("[resource: {uri}]"));
                }
            }
        }
    }

    serde_json::json!({
        "content": texts,
        "is_error": result.is_error,
        "text": texts.join("\n"),
    })
}

// ── 动态加载入口 ──────────────────────────────────────

#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    Box::new(SimpleCadPlugin::new())
}

// ── 单元测试 ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_config_default() {
        let config = SimpleCadConfig::default();
        assert_eq!(config.python_cmd, "python3");
        assert_eq!(config.server_module, "simplecad_mcp.server");
        assert_eq!(config.tool_timeout_ms, 120_000);
        assert_eq!(config.max_restarts, 2);
    }

    #[test]
    fn test_config_custom() {
        let config = SimpleCadConfig {
            python_cmd: "uv".into(),
            server_module: "run simplecad-mcp".into(),
            tool_timeout_ms: 60_000,
            max_restarts: 5,
            ..Default::default()
        };
        assert_eq!(config.python_cmd, "uv");
        assert_eq!(config.tool_timeout_ms, 60_000);
        assert_eq!(config.max_restarts, 5);
    }

    #[test]
    fn test_plugin_info() {
        let plugin = SimpleCadPlugin::new();
        let info = plugin.info();
        assert_eq!(info.manifest.name, "simplecad-plugin");
        assert_eq!(info.manifest.version, "0.1.0");
        assert_eq!(info.manifest.capabilities.len(), 3);
    }

    #[test]
    fn test_plugin_capabilities() {
        let plugin = SimpleCadPlugin::new();
        let info = plugin.info();
        let names: Vec<&str> = info.manifest.capabilities
            .iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"cad-modeling"));
        assert!(names.contains(&"cad-export"));
        assert!(names.contains(&"cad-replay"));
    }

    #[test]
    fn test_plugin_permissions() {
        let plugin = SimpleCadPlugin::new();
        let perms = plugin.required_permissions();
        assert!(perms.iter().any(|p| p.resource == "process"));
        assert!(perms.iter().any(|p| p.resource == "filesystem"));
    }

    #[test]
    fn test_as_any_downcast() {
        let plugin = SimpleCadPlugin::new();
        let any = plugin.as_any();
        let downcast = any.downcast_ref::<SimpleCadPlugin>();
        assert!(downcast.is_some());
    }

    #[test]
    fn test_plugin_default() {
        let plugin: SimpleCadPlugin = Default::default();
        assert_eq!(plugin.info().manifest.name, "simplecad-plugin");
    }

    #[test]
    fn test_extract_params_empty() {
        let schema = serde_json::json!({});
        let params = extract_params(&schema);
        assert!(params.is_empty());
    }

    #[test]
    fn test_extract_params_with_properties() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "radius": { "type": "number", "description": "半径" },
                "height": { "type": "number", "description": "高度" }
            },
            "required": ["radius"]
        });
        let params = extract_params(&schema);
        assert_eq!(params.len(), 2);
        assert!(params.iter().any(|p| p.name == "radius" && p.required));
        assert!(params.iter().any(|p| p.name == "height" && !p.required));
    }

    #[test]
    fn test_mcp_result_to_value_text() {
        let result = lingshu_mcp::rmcp_stdio_client::McpToolResult {
            content: vec![
                lingshu_mcp::rmcp_stdio_client::McpContent::Text {
                    text: r#"{"status":"ok","tag":"shape_0"}"#.into(),
                },
            ],
            is_error: false,
        };
        let val = mcp_result_to_value(result);
        assert_eq!(val["is_error"], false);
        assert!(!val["text"].as_str().unwrap_or("").is_empty());
    }
}
