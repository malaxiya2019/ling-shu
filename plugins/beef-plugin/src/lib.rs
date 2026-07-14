//! 🕷️ BeEF Browser Exploitation Framework — LingShu Security Testing Plugin.
//!
//! 将 BeEF (Browser Exploitation Framework) 打包为 LingShu 的安全测试插件。
//! 管理 BeEF 子进程生命周期，提供 REST API 代理，集成安全测试工作流。
//!
//! ## 功能
//!
//! - 自动管理 BeEF Ruby 子进程 (start/stop/health check)
//! - 通过 REST API 控制 hooked browsers
//! - 安全测试模块执行
//! - 事件通知集成 (hooked browser 上线/下线)
//!
//! ## 用法
//!
//! ```ignore
//! // 作为静态插件注册
//! let plugin = lingshu_beef_plugin::BeefPlugin::new("/path/to/beef", 3000);
//! registry.register(Box::new(plugin), None).await?;
//! ```

pub mod api;
pub mod manager;

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::plugin::{Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus};

/// BeEF 安全测试插件.
pub struct BeefPlugin {
    info: PluginInfo,
    manager: Arc<BeefManager>,
    config: BeefConfig,
}

/// 插件配置.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BeefConfig {
    /// BeEF 源码目录 (含 config.yaml, beef 可执行文件).
    pub beef_dir: PathBuf,
    /// Ruby 可执行文件路径.
    pub ruby_bin: String,
    /// BeEF HTTP 端口.
    pub port: u16,
    /// BeEF 管理界面用户名.
    pub username: String,
    /// BeEF 管理界面密码.
    pub password: String,
    /// 自动重启最大次数.
    pub max_restarts: u32,
    /// 安全测试标签.
    pub tags: Vec<String>,
}

impl Default for BeefConfig {
    fn default() -> Self {
        Self {
            beef_dir: PathBuf::from("beef"),
            ruby_bin: "ruby".to_string(),
            port: 3000,
            username: "beef".to_string(),
            password: "beef".to_string(),
            max_restarts: 3,
            tags: vec![
                "security".into(),
                "browser-exploitation".into(),
                "xss-testing".into(),
                "phishing".into(),
            ],
        }
    }
}

impl BeefPlugin {
    /// 创建 BeEF 插件实例.
    pub fn new(beef_dir: PathBuf) -> Self {
        let config = BeefConfig {
            beef_dir: beef_dir.clone(),
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// 使用自定义配置创建 BeEF 插件.
    pub fn with_config(config: BeefConfig) -> Self {
        let manager = BeefManager::new(config.beef_dir.clone(), &config.ruby_bin, config.port);

        let manifest = PluginManifest {
            name: "beef-plugin".into(),
            version: "1.0.0".into(),
            description: "BeEF Browser Exploitation Framework — 浏览器安全测试与利用框架".into(),
            author: Some("Lingshu Team".into()),
            homepage: Some("https://beefproject.com".into()),
            license: Some("Apache-2.0".into()),
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![
                PluginPermission {
                    resource: "process".into(),
                    actions: vec!["spawn".into(), "kill".into()],
                },
                PluginPermission {
                    resource: "network".into(),
                    actions: vec!["http".into()],
                },
                PluginPermission {
                    resource: "file".into(),
                    actions: vec!["read".into()],
                },
            ],
            min_api_version: Some("1.0.0".into()),
        ..Default::default()
        };

        let info = PluginInfo {
            plugin_id: LsId::new(),
            manifest,
            status: PluginStatus::Installed,
            loaded_at: None,
        };

        Self {
            info,
            manager: Arc::new(manager),
            config,
        }
    }

    /// 获取 BeEF 管理器引用.
    pub fn manager(&self) -> &Arc<BeefManager> {
        &self.manager
    }

    /// 获取 BeEF API 客户端 (需先启动 BeEF).
    pub async fn client(&self) -> Result<api::BeefClient, String> {
        let base_url = format!("http://127.0.0.1:{}", self.config.port);
        let mut client = api::BeefClient::new(&base_url);
        client
            .login(&self.config.username, &self.config.password)
            .await?;
        Ok(client)
    }

    /// 获取插件运行时状态（含 BeEF 子进程状态）.
    pub async fn plugin_status(&self) -> serde_json::Value {
        let beef_status = self.manager.status().await;
        serde_json::json!({
            "plugin_id": self.info.plugin_id.to_string(),
            "name": self.info.manifest.name,
            "status": format!("{:?}", self.info.status),
            "beef": {
                "status": format!("{:?}", beef_status),
                "port": self.config.port,
                "dir": self.config.beef_dir.to_str(),
                "ruby": self.config.ruby_bin,
            },
            "config": {
                "username": self.config.username,
                "port": self.config.port,
                "tags": self.config.tags,
            }
        })
    }
}

#[async_trait]
impl Plugin for BeefPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(
            plugin = "beef-plugin",
            beef_dir = %self.config.beef_dir.display(),
            "BeEF plugin initialized"
        );
        Ok(())
    }

    async fn start(&self, ctx: LsContext) -> LsResult<()> {
        tracing::info!(
            plugin = "beef-plugin",
            session = %ctx.session_id,
            "Starting BeEF process..."
        );

        // 启动 BeEF 子进程
        self.manager
            .start()
            .await
            .map_err(|e| lingshu_core::LsError::Plugin(format!("BeEF start failed: {}", e)))?;

        // 更新插件状态
        tracing::info!(
            plugin = "beef-plugin",
            port = self.config.port,
            "BeEF is running"
        );
        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "beef-plugin", "Stopping BeEF process...");
        self.manager
            .stop()
            .await
            .map_err(|e| lingshu_core::LsError::Plugin(format!("BeEF stop failed: {}", e)))?;
        tracing::info!(plugin = "beef-plugin", "BeEF stopped");
        Ok(())
    }

        fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn required_permissions(&self) -> Vec<PluginPermission> {
        self.info.manifest.permissions.clone()
    }
}

// ── Re-exports ──────────────────────────────────────

pub use api::{
    BeefClient, BeefLogEntry, BeefLogin, BeefModule, HookedBrowser, ModuleExecRequest,
    ModuleExecResponse,
};
pub use manager::BeefManager;
pub use manager::BeefStatus;

/// 创建一个预配置的 BeEF 插件实例（用于动态加载入口）.
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    Box::new(BeefPlugin::new(PathBuf::from("beef")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_info() {
        let plugin = BeefPlugin::new(PathBuf::from("/tmp/beef"));
        let info = plugin.info();
        assert_eq!(info.manifest.name, "beef-plugin");
        assert_eq!(info.manifest.plugin_type, "static");
    }

    #[test]
    fn test_plugin_permissions() {
        let plugin = BeefPlugin::new(PathBuf::from("/tmp/beef"));
        let perms = plugin.required_permissions();
        assert!(perms.iter().any(|p| p.resource == "process"));
        assert!(perms.iter().any(|p| p.resource == "network"));
    }

    #[test]
    fn test_config_default() {
        let config = BeefConfig::default();
        assert_eq!(config.port, 3000);
        assert_eq!(config.username, "beef");
        assert_eq!(config.ruby_bin, "ruby");
    }

    #[tokio::test]
    async fn test_plugin_status_not_started() {
        let plugin = BeefPlugin::new(PathBuf::from("/tmp/beef"));
        let status = plugin.plugin_status().await;
        assert_eq!(status["name"], "beef-plugin");
        assert!(status["beef"]["status"]
            .as_str()
            .unwrap()
            .contains("Stopped"));
    }

    #[tokio::test]
    #[ignore = "reqwest HTTP client crashes in Termux (SIGSEGV)"]
    async fn test_beef_client_connect_refused() {
        // 在没有 BeEF 服务时，is_alive 返回 false
        let client = BeefClient::new("http://127.0.0.1:19999");
        let result = client.is_alive().await;
        assert!(
            !result,
            "Expected health check to fail when no BeEF is running"
        );
    }
}
