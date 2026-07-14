//! 📹 Watch Skill — LingShu Video Analysis Plugin.
//!
//! 将 watch-skill (Python) 打包为 LingShu 的插件。
//! 管理 watch-skill API 子进程生命周期，提供视频分析能力。
//!
//! ## 功能
//!
//! - 自动管理 watch-skill API 子进程 (start/stop/health check)
//! - 观看视频 (YouTube, 本地文件, HLS/DASH 等 1800+ 来源)
//! - 基于索引的问答 (持久化 SQLite)
//! - 跨视频搜索 (keyword + semantic)
//! - 屏幕/UI 录制分析 (THE LOOP)
//!
//! ## 用法
//!
//! ```ignore
//! let plugin = lingshu_watch_plugin::WatchPlugin::new("python3", 8748);
//! registry.register(Box::new(plugin), None).await?;
//! ```

pub mod api;
pub mod manager;

use async_trait::async_trait;
use std::sync::Arc;

use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::plugin::{Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus};

/// Watch Skill 插件.
pub struct WatchPlugin {
    info: PluginInfo,
    manager: Arc<WatchManager>,
    config: WatchConfig,
}

/// 插件配置.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WatchConfig {
    /// Python 可执行文件路径.
    pub python_bin: String,
    /// Watch Skill API 端口.
    pub port: u16,
    /// 视频帧预算上限.
    pub max_frames: u32,
    /// 是否启用 THE LOOP.
    pub loop_enabled: bool,
    /// 自动重启最大次数.
    pub max_restarts: u32,
    /// 标签.
    pub tags: Vec<String>,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            python_bin: "python3".to_string(),
            port: 8748,
            max_frames: 100,
            loop_enabled: true,
            max_restarts: 3,
            tags: vec![
                "video".into(),
                "analysis".into(),
                "vision".into(),
                "testing".into(),
                "automation".into(),
            ],
        }
    }
}

impl WatchPlugin {
    /// 创建 Watch Skill 插件实例.
    pub fn new(python_bin: impl Into<String>) -> Self {
        let config = WatchConfig {
            python_bin: python_bin.into(),
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// 使用自定义配置创建.
    pub fn with_config(config: WatchConfig) -> Self {
        let manager = WatchManager::new(&config.python_bin, config.port);

        let manifest = PluginManifest {
            name: "watch-plugin".into(),
            version: "1.0.0".into(),
            description: "Watch Skill — 视频分析插件。观看视频、问答、跨视频搜索、UI录制分析"
                .into(),
            author: Some("Lingshu Team".into()),
            homepage: Some("https://github.com/oxbshw/watch-skill".into()),
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
                    actions: vec!["http".into()],
                },
                PluginPermission {
                    resource: "video".into(),
                    actions: vec!["watch".into(), "analyze".into()],
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

    /// 获取 Watch Manager 引用.
    pub fn manager(&self) -> &Arc<WatchManager> {
        &self.manager
    }

    /// 创建 API 客户端 (需先启动服务).
    pub fn client(&self) -> api::WatchClient {
        let base_url = format!("http://127.0.0.1:{}", self.config.port);
        api::WatchClient::new(&base_url)
    }

    /// 获取插件运行时状态.
    pub async fn plugin_status(&self) -> serde_json::Value {
        let ws_status = self.manager.status().await;
        serde_json::json!({
            "plugin_id": self.info.plugin_id.to_string(),
            "name": self.info.manifest.name,
            "status": format!("{:?}", self.info.status),
            "watch_skill": {
                "status": format!("{:?}", ws_status),
                "port": self.config.port,
                "python": self.config.python_bin,
            },
            "config": {
                "port": self.config.port,
                "max_frames": self.config.max_frames,
                "loop_enabled": self.config.loop_enabled,
                "tags": self.config.tags,
            }
        })
    }
}

#[async_trait]
impl Plugin for WatchPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(
            plugin = "watch-plugin",
            port = self.config.port,
            python = %self.config.python_bin,
            "Watch Skill plugin initialized"
        );
        Ok(())
    }

    async fn start(&self, ctx: LsContext) -> LsResult<()> {
        tracing::info!(
            plugin = "watch-plugin",
            session = %ctx.session_id,
            "Starting watch-skill API server..."
        );

        self.manager.start().await.map_err(|e| {
            lingshu_core::LsError::Plugin(format!("Watch Skill start failed: {}", e))
        })?;

        tracing::info!(
            plugin = "watch-plugin",
            port = self.config.port,
            "Watch Skill API is running"
        );
        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(
            plugin = "watch-plugin",
            "Stopping watch-skill API server..."
        );
        self.manager.stop().await.map_err(|e| {
            lingshu_core::LsError::Plugin(format!("Watch Skill stop failed: {}", e))
        })?;
        tracing::info!(plugin = "watch-plugin", "Watch Skill stopped");
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
    AskRequest, AskResponse, CaptureRequest, CaptureResponse, Evidence, FrameInfo, LoopResponse,
    LoopStartRequest, SearchResult, VideoItem, WatchClient, WatchRequest, WatchResponse,
};
pub use manager::WatchManager;
pub use manager::WatchStatus;
