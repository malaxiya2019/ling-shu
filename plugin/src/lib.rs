//! 🧩 Lingshu Plugin Runtime (Phase 8 + v3.0)
//!
//! 提供插件注册、加载、沙箱隔离与生命周期管理能力。
//!
//! ## 核心组件
//!
//! - [`PluginRegistry`] — 线程安全的插件注册中心
//! - [`loader::PluginLoader`] — 静态/动态插件加载器
//! - [`sandbox::check_permission`] — 权限检查沙箱

pub mod loader;
pub mod sandbox;
pub mod manifest;
pub mod market;
pub mod hot_reload;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use manifest::{
    DependencyResolver, ExtendedManifest, MarketMeta, PluginDependency,
    PluginDepType, VersionCompat,
};
pub use market::{InstallOptions, MarketPluginEntry, MarketSearchResult, PluginMarket, RegistrySource};
pub use hot_reload::{HotReloadEvent, HotReloadWatcher};

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::plugin::{Plugin, PluginInfo, PluginStatus};
use tokio::sync::RwLock;
use tracing::info;

/// 插件注册中心 — 线程安全的插件存储与生命周期管理.
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<LsId, RegistryEntry>>>,
}

/// 注册中心中的插件条目.
struct RegistryEntry {
    pub info: PluginInfo,
    pub plugin: Box<dyn Plugin>,
    pub _lib: Option<libloading::Library>, // 保持动态库不被卸载
}

impl PluginRegistry {
    /// 创建一个空的插件注册中心.
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册一个插件 (静态或动态).
    pub async fn register(
        &self,
        plugin: Box<dyn Plugin>,
        lib: Option<libloading::Library>,
    ) -> LsResult<LsId> {
        let mut info = plugin.info();
        let plugin_id = info.plugin_id;
        let mut map = self.plugins.write().await;

        if map.contains_key(&plugin_id) {
            return Err(LsError::AlreadyExists(format!(
                "plugin '{}' already registered",
                info.manifest.name
            )));
        }

        info.status = PluginStatus::Installed;
        info.loaded_at = Some(Utc::now());

        map.insert(
            plugin_id,
            RegistryEntry {
                info,
                plugin,
                _lib: lib,
            },
        );

        info!(plugin_id = %plugin_id, "plugin registered");
        Ok(plugin_id)
    }

    /// 按 ID 获取插件信息.
    pub async fn get_info(&self, plugin_id: &LsId) -> LsResult<PluginInfo> {
        let map = self.plugins.read().await;
        map.get(plugin_id)
            .map(|e| e.info.clone())
            .ok_or_else(|| LsError::PluginNotFound(plugin_id.to_string()))
    }

    /// 列出所有已注册插件.
    pub async fn list(&self) -> Vec<PluginInfo> {
        let map = self.plugins.read().await;
        map.values().map(|e| e.info.clone()).collect()
    }

    /// 按名称/ID 前缀搜索插件.
    pub async fn search(&self, query: &str) -> Vec<PluginInfo> {
        let map = self.plugins.read().await;
        map.values()
            .filter(|e| {
                e.info.manifest.name.contains(query) || e.info.plugin_id.to_string().contains(query)
            })
            .map(|e| e.info.clone())
            .collect()
    }

    /// 初始化插件.
    pub async fn init_plugin(&self, plugin_id: &LsId, ctx: &LsContext) -> LsResult<()> {
        let mut map = self.plugins.write().await;
        let entry = map
            .get_mut(plugin_id)
            .ok_or_else(|| LsError::PluginNotFound(plugin_id.to_string()))?;

        if entry.info.status != PluginStatus::Installed && entry.info.status != PluginStatus::Loaded
        {
            return Err(LsError::Plugin(format!(
                "plugin '{}' cannot be initialized from state {:?}",
                entry.info.manifest.name, entry.info.status
            )));
        }

        // 权限检查
        for perm in entry.plugin.required_permissions() {
            for action in &perm.actions {
                sandbox::check_permission(&entry.info.manifest, &perm.resource, action)
                    .map_err(|e| {
                        tracing::warn!(name = %entry.info.manifest.name, error = %e, "permission check failed");
                        e
                    })?;
            }
        }

        entry.plugin.init(ctx.clone()).await?;
        entry.info.status = PluginStatus::Loaded;
        info!(plugin_id = %plugin_id, "plugin initialized");
        Ok(())
    }

    /// 启动插件.
    pub async fn start_plugin(&self, plugin_id: &LsId, ctx: &LsContext) -> LsResult<()> {
        let mut map = self.plugins.write().await;
        let entry = map
            .get_mut(plugin_id)
            .ok_or_else(|| LsError::PluginNotFound(plugin_id.to_string()))?;

        if entry.info.status != PluginStatus::Loaded {
            return Err(LsError::Plugin(format!(
                "plugin '{}' not loaded (state: {:?})",
                entry.info.manifest.name, entry.info.status
            )));
        }

        entry.plugin.start(ctx.clone()).await?;
        entry.info.status = PluginStatus::Running;
        info!(plugin_id = %plugin_id, "plugin started");
        Ok(())
    }

    /// 停止插件.
    pub async fn stop_plugin(&self, plugin_id: &LsId, ctx: &LsContext) -> LsResult<()> {
        let mut map = self.plugins.write().await;
        let entry = map
            .get_mut(plugin_id)
            .ok_or_else(|| LsError::PluginNotFound(plugin_id.to_string()))?;

        if entry.info.status != PluginStatus::Running {
            return Err(LsError::Plugin(format!(
                "plugin '{}' is not running (state: {:?})",
                entry.info.manifest.name, entry.info.status
            )));
        }

        entry.plugin.stop(ctx.clone()).await?;
        entry.info.status = PluginStatus::Stopped;
        info!(plugin_id = %plugin_id, "plugin stopped");
        Ok(())
    }

    /// 卸载插件.
    pub async fn unregister(&self, plugin_id: &LsId) -> LsResult<()> {
        let mut map = self.plugins.write().await;
        map.remove(plugin_id)
            .ok_or_else(|| LsError::PluginNotFound(plugin_id.to_string()))?;
        info!(plugin_id = %plugin_id, "plugin unregistered");
        Ok(())
    }

    /// 对插件执行一个闭包操作 (内部使用).
    pub async fn with_plugin<F, R>(&self, plugin_id: &LsId, f: F) -> LsResult<R>
    where
        F: FnOnce(&dyn Plugin) -> LsResult<R>,
    {
        let map = self.plugins.read().await;
        let entry = map
            .get(plugin_id)
            .ok_or_else(|| LsError::PluginNotFound(plugin_id.to_string()))?;
        f(entry.plugin.as_ref())
    }

    /// 已注册插件数量.
    pub async fn count(&self) -> usize {
        self.plugins.read().await.len()
    }

    /// 注册插件并检查依赖.
    pub async fn register_with_deps(
        &self,
        plugin: Box<dyn Plugin>,
        lib: Option<libloading::Library>,
        deps: &[PluginDependency],
    ) -> LsResult<LsId> {
        let mut installed = std::collections::HashMap::new();
        let plugins = self.plugins.read().await;
        for entry in plugins.values() {
            installed.insert(
                entry.info.manifest.name.clone(),
                entry.info.manifest.version.clone(),
            );
        }
        drop(plugins);

        let unsatisfied = DependencyResolver::check_dependencies(&installed, deps);
        if !unsatisfied.is_empty() {
            let missing: Vec<String> = unsatisfied
                .iter()
                .map(|d| format!("{} ({})", d.name, d.version_req))
                .collect();
            return Err(LsError::Plugin(format!(
                "unsatisfied dependencies: {}",
                missing.join(", ")
            )));
        }

        self.register(plugin, lib).await
    }

    /// 获取指定名称的插件信息.
    pub async fn get_info_by_name(&self, name: &str) -> LsResult<PluginInfo> {
        let map = self.plugins.read().await;
        for entry in map.values() {
            if entry.info.manifest.name == name {
                return Ok(entry.info.clone());
            }
        }
        Err(LsError::PluginNotFound(name.to_string()))
    }

    /// 按版本过滤已注册插件.
    pub async fn list_by_version(&self, version_req: &str) -> Vec<PluginInfo> {
        let req = semver::VersionReq::parse(version_req).ok();
        let map = self.plugins.read().await;
        map.values()
            .filter(|entry| {
                if let Some(ref req) = req {
                    if let Ok(v) = semver::Version::parse(&entry.info.manifest.version) {
                        return req.matches(&v);
                    }
                }
                false
            })
            .map(|entry| entry.info.clone())
            .collect()
    }

    /// 批量注册插件.
    pub async fn register_batch(
        &self,
        plugins: Vec<(Box<dyn Plugin>, Option<libloading::Library>)>,
    ) -> LsResult<Vec<LsId>> {
        let mut ids = Vec::with_capacity(plugins.len());
        for (plugin, lib) in plugins {
            ids.push(self.register(plugin, lib).await?);
        }
        Ok(ids)
    }
}

// ── StaticPlugin ────────────────────────────────────

/// 一个简单的静态插件包装，可直接从 PluginInfo 构建.
pub struct StaticPlugin {
    info: PluginInfo,
}

impl StaticPlugin {
    pub fn new(info: PluginInfo) -> Self {
        Self { info }
    }
}

#[async_trait]
impl Plugin for StaticPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        Ok(())
    }

    async fn start(&self, _ctx: LsContext) -> LsResult<()> {
        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        Ok(())
    }

    fn required_permissions(&self) -> Vec<lingshu_traits::plugin::PluginPermission> {
        self.info.manifest.permissions.clone()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_traits::plugin::{
        Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus,
    };

    struct TestPlugin {
        info: PluginInfo,
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn info(&self) -> PluginInfo {
            self.info.clone()
        }

        async fn init(&self, _ctx: LsContext) -> LsResult<()> {
            Ok(())
        }

        async fn start(&self, _ctx: LsContext) -> LsResult<()> {
            Ok(())
        }

        async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
            Ok(())
        }

        fn required_permissions(&self) -> Vec<PluginPermission> {
            vec![]
        }
    }

    fn make_plugin(name: &str) -> Box<dyn Plugin> {
        let manifest = PluginManifest {
            name: name.into(),
            version: "1.0.0".into(),
            description: "test plugin".into(),
            author: None,
            homepage: None,
            license: None,
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![],
            min_api_version: None,
        };
        let info = PluginInfo {
            plugin_id: LsId::new(),
            manifest,
            status: PluginStatus::Installed,
            loaded_at: None,
        };
        Box::new(TestPlugin { info })
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let registry = PluginRegistry::new();
        let id = registry
            .register(make_plugin("test-1"), None)
            .await
            .unwrap();
        assert!(!id.is_nil());

        let list = registry.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].manifest.name, "test-1");
    }

    #[tokio::test]
    async fn test_duplicate_register() {
        let registry = PluginRegistry::new();
        let manifest = PluginManifest {
            name: "dup".into(),
            version: "1.0.0".into(),
            description: "dup plugin".into(),
            author: None,
            homepage: None,
            license: None,
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![],
            min_api_version: None,
        };
        let info = PluginInfo {
            plugin_id: LsId::new(),
            manifest,
            status: PluginStatus::Installed,
            loaded_at: None,
        };
        let first: Box<dyn Plugin> = Box::new(TestPlugin { info });
        let pid = first.info().plugin_id;
        registry.register(first, None).await.unwrap();

        // 尝试注册同 ID 的第二个插件
        let dup_info = PluginInfo {
            plugin_id: pid,
            manifest: PluginManifest {
                name: "dup2".into(),
                ..PluginManifest::default()
            },
            ..PluginInfo::default()
        };
        let dup: Box<dyn Plugin> = Box::new(TestPlugin { info: dup_info });
        let result = registry.register(dup, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_info_not_found() {
        let registry = PluginRegistry::new();
        let result = registry.get_info(&LsId::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lifecycle() {
        let registry = PluginRegistry::new();
        let ctx = LsContext::with_session(LsId::new());
        let id = registry
            .register(make_plugin("lifecycle"), None)
            .await
            .unwrap();

        registry.init_plugin(&id, &ctx).await.unwrap();
        let info = registry.get_info(&id).await.unwrap();
        assert_eq!(info.status, PluginStatus::Loaded);

        registry.start_plugin(&id, &ctx).await.unwrap();
        let info = registry.get_info(&id).await.unwrap();
        assert_eq!(info.status, PluginStatus::Running);

        registry.stop_plugin(&id, &ctx).await.unwrap();
        let info = registry.get_info(&id).await.unwrap();
        assert_eq!(info.status, PluginStatus::Stopped);
    }

    #[tokio::test]
    async fn test_unregister() {
        let registry = PluginRegistry::new();
        let id = registry
            .register(make_plugin("remove"), None)
            .await
            .unwrap();
        assert_eq!(registry.count().await, 1);
        registry.unregister(&id).await.unwrap();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_search() {
        let registry = PluginRegistry::new();
        registry
            .register(make_plugin("alpha-plugin"), None)
            .await
            .unwrap();
        registry
            .register(make_plugin("beta-tool"), None)
            .await
            .unwrap();
        registry.register(make_plugin("gamma"), None).await.unwrap();

        let results = registry.search("plugin").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].manifest.name, "alpha-plugin");
    }
}
