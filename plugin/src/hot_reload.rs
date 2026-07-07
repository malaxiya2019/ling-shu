//! 🔄 Hot Reload — 插件热加载与文件系统监控.
//!
//! 使用 `notify` crate 监控插件目录，在文件变化时自动重新加载插件。
//! 支持优雅的错误恢复：新版本加载失败时保留旧版本继续运行。

use crate::loader::PluginLoader;
use crate::PluginRegistry;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::plugin::{Plugin, PluginInfo, PluginStatus};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

/// 插件热重载事件.
#[derive(Debug, Clone)]
pub enum HotReloadEvent {
    /// 插件已重新加载.
    Reloaded {
        plugin_id: LsId,
        name: String,
        version: String,
    },
    /// 插件加载失败 (旧版本保留).
    Failed {
        name: String,
        error: String,
    },
    /// 插件已移除.
    Removed {
        name: String,
    },
}

/// 插件热重载监控器.
pub struct HotReloadWatcher {
    /// 被监控的目录.
    watch_dir: PathBuf,
    /// 通知渠道.
    event_tx: mpsc::Sender<HotReloadEvent>,
    /// 是否正在运行.
    running: Arc<RwLock<bool>>,
}

impl HotReloadWatcher {
    /// 创建新的热重载监控器.
    pub fn new(watch_dir: PathBuf) -> Self {
        let (event_tx, _) = mpsc::channel(256);
        Self {
            watch_dir,
            event_tx,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动文件系统监控.
    ///
    /// 在单独的线程中运行 `notify` 文件监控，通过 channel 向异步运行时
    /// 发送文件变更事件，触发插件热重载。
    pub async fn start(
        &self,
        registry: Arc<PluginRegistry>,
        loader: Arc<PluginLoader>,
        ctx: LsContext,
        on_event: Arc<dyn Fn(HotReloadEvent) + Send + Sync + 'static>,
    ) -> LsResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(LsError::Plugin("hot-reload watcher already running".into()));
        }
        *running = true;
        drop(running);

        let watch_dir = self.watch_dir.clone();
        let (tx, mut rx) = mpsc::channel::<Result<Event, notify::Error>>(256);
        let event_tx = self.event_tx.clone();

        // 启动 notify watcher (在阻塞线程中运行)
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.blocking_send(res);
            },
            Config::default(),
        )
        .map_err(|e| LsError::Plugin(format!("failed to create file watcher: {e}")))?;

        watcher
            .watch(&watch_dir, RecursiveMode::Recursive)
            .map_err(|e| {
                LsError::Plugin(format!(
                    "failed to watch directory '{}': {e}",
                    watch_dir.display()
                ))
            })?;

        info!(
            dir = %watch_dir.display(),
            "hot-reload watcher started"
        );

        let running_flag = self.running.clone();

        // 异步处理文件事件
        tokio::spawn(async move {
            let mut debounce: std::collections::HashMap<PathBuf, tokio::time::Instant> =
                std::collections::HashMap::new();

            while let Some(Ok(event)) = rx.recv().await {
                // 检查是否仍在运行
                if !*running_flag.read().await {
                    break;
                }

                match event.kind {
                    EventKind::Create(_)
                    | EventKind::Modify(_)
                    | EventKind::Remove(_) => {
                        for path in &event.paths {
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("");

                            let should_watch = matches!(
                                ext,
                                "so" | "dylib" | "wasm" | "json" | "plugin"
                            );

                            if !should_watch {
                                continue;
                            }

                            let now = tokio::time::Instant::now();
                            if let Some(last) = debounce.get(path) {
                                if now.duration_since(*last) < tokio::time::Duration::from_secs(2)
                                {
                                    continue;
                                }
                            }
                            debounce.insert(path.clone(), now);

                            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

                            info!(
                                path = %path.display(),
                                "plugin file change detected"
                            );

                            match event.kind {
                                EventKind::Remove(_) => {
                                    let plugin_name = path
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("unknown");

                                    let plugins = registry.list().await;
                                    for p in &plugins {
                                        if p.manifest.name == plugin_name
                                            || p.manifest
                                                .entry_point
                                                .as_ref()
                                                .map(|e| path.ends_with(e))
                                                .unwrap_or(false)
                                        {
                                            let _ =
                                                registry.unregister(&p.plugin_id).await;
                                            let event = HotReloadEvent::Removed {
                                                name: plugin_name.to_string(),
                                            };
                                            on_event(event.clone());
                                            let _ = event_tx.send(event).await;
                                            break;
                                        }
                                    }
                                }
                                _ => {
                                    if let Err(e) = Self::reload_plugin(
                                        &registry,
                                        &loader,
                                        &ctx,
                                        path,
                                        &on_event,
                                        &event_tx,
                                    )
                                    .await
                                    {
                                        warn!(
                                            path = %path.display(),
                                            error = %e,
                                            "hot-reload failed for plugin"
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            info!("hot-reload watcher stopped");
        });

        Ok(())
    }

    /// 停止文件系统监控.
    pub async fn stop(&self) -> LsResult<()> {
        let mut running = self.running.write().await;
        *running = false;
        info!("hot-reload watcher stopping");
        Ok(())
    }

    /// 检查 watcher 是否正在运行.
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// 获取事件接收器.
    pub fn subscribe(&self) -> mpsc::Receiver<HotReloadEvent> {
        let (_tx, rx) = mpsc::channel(256);
        rx
    }

    /// 重新加载单个插件.
    async fn reload_plugin(
        registry: &PluginRegistry,
        loader: &PluginLoader,
        ctx: &LsContext,
        path: &Path,
        on_event: &Arc<dyn Fn(HotReloadEvent) + Send + Sync + 'static>,
        event_tx: &mpsc::Sender<HotReloadEvent>,
    ) -> LsResult<()> {
        let plugin_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // 查找已注册的旧版本插件
        let old_plugin_id: Option<LsId> = {
            let plugins = registry.list().await;
            plugins
                .iter()
                .find(|p| {
                    p.manifest.name == plugin_name
                        || p.manifest
                            .entry_point
                            .as_ref()
                            .map(|e| path.ends_with(e))
                            .unwrap_or(false)
                })
                .map(|p| p.plugin_id)
        };

        // 停止旧插件
        if let Some(id) = old_plugin_id {
            if let Ok(info) = registry.get_info(&id).await {
                if info.status == PluginStatus::Running {
                    let _ = registry.stop_plugin(&id, ctx).await;
                }
            }
        }

        // 加载新版本
        let result = if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let manifest_dir = path.parent().unwrap_or(Path::new("."));
            let manifest = crate::manifest::load_manifest_from_path(path)?;

            let lib_paths = [
                manifest_dir.join(format!("{}.so", manifest.base.name)),
                manifest_dir.join(format!("{}.dylib", manifest.base.name)),
                manifest_dir.join(format!("{}.wasm", manifest.base.name)),
            ];

            let lib_path = lib_paths.iter().find(|p| p.exists()).cloned();

            if let Some(lib_path) = lib_path {
                let (info, lib) = unsafe { loader.load_dynamic(&lib_path)? };
                let plugin_id = registry
                    .register(
                        create_dynamic_plugin(&lib, &info)?,
                        Some(lib),
                    )
                    .await?;
                registry.init_plugin(&plugin_id, ctx).await?;
                registry.start_plugin(&plugin_id, ctx).await?;
                Ok(plugin_id)
            } else {
                Err(LsError::Plugin(format!(
                    "no library file found for plugin '{}' in '{}'",
                    manifest.base.name,
                    manifest_dir.display()
                )))
            }
        } else {
            let (info, lib) = unsafe { loader.load_dynamic(path)? };
            let plugin_id = registry
                .register(
                    create_dynamic_plugin(&lib, &info)?,
                    Some(lib),
                )
                .await?;
            registry.init_plugin(&plugin_id, ctx).await?;
            registry.start_plugin(&plugin_id, ctx).await?;
            Ok(plugin_id)
        };

        match result {
            Ok(plugin_id) => {
                let info = registry.get_info(&plugin_id).await?;
                info!(
                    name = %info.manifest.name,
                    version = %info.manifest.version,
                    "plugin hot-reloaded successfully"
                );

                let event = HotReloadEvent::Reloaded {
                    plugin_id,
                    name: info.manifest.name.clone(),
                    version: info.manifest.version.clone(),
                };
                on_event(event.clone());
                let _ = event_tx.send(event).await;
                Ok(())
            }
            Err(e) => {
                if let Some(id) = old_plugin_id {
                    if let Ok(info) = registry.get_info(&id).await {
                        let _ = registry.init_plugin(&id, ctx).await;
                        let _ = registry.start_plugin(&id, ctx).await;
                        warn!(
                            name = %info.manifest.name,
                            "kept old version after hot-reload failure"
                        );
                    }
                }

                let event = HotReloadEvent::Failed {
                    name: plugin_name,
                    error: e.to_string(),
                };
                on_event(event.clone());
                let _ = event_tx.send(event).await;
                Err(e)
            }
        }
    }
}

/// 从动态库创建插件实例 (辅助函数).
fn create_dynamic_plugin(
    lib: &libloading::Library,
    _info: &PluginInfo,
) -> LsResult<Box<dyn Plugin>> {
    unsafe {
        let create: libloading::Symbol<unsafe extern "C" fn() -> Box<dyn Plugin>> = lib
            .get(b"create_plugin")
            .map_err(|e| {
                LsError::Plugin(format!("symbol 'create_plugin' not found: {e}"))
            })?;
        Ok(create())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;

    #[test]
    fn test_hot_reload_watcher_creation() {
        let watcher = HotReloadWatcher::new(PathBuf::from("/tmp/plugins"));
        assert!(!watcher.watch_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_hot_reload_event_serialization() {
        let event = HotReloadEvent::Reloaded {
            plugin_id: LsId::new(),
            name: "test".into(),
            version: "1.0.0".into(),
        };
        match event {
            HotReloadEvent::Reloaded { name, version, .. } => {
                assert_eq!(name, "test");
                assert_eq!(version, "1.0.0");
            }
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn test_should_watch_extensions() {
        let check_ext = |name: &str| -> bool {
            let path = Path::new(name);
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            matches!(ext, "so" | "dylib" | "wasm" | "json" | "plugin")
        };

        assert!(check_ext("plugin.so"));
        assert!(check_ext("plugin.dylib"));
        assert!(check_ext("plugin.wasm"));
        assert!(check_ext("plugin.json"));
        assert!(check_ext("plugin.plugin"));
        assert!(!check_ext("plugin.txt"));
        assert!(!check_ext("plugin.rs"));
    }
}
