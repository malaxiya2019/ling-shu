//! 🧩 Lingshu Plugin Runtime (v3.8)
//!
//! 提供插件注册、加载、沙箱隔离与生命周期管理能力。
//! 集成 ToolRegistry，支持插件自动注册工具。
//!
//! ## 核心组件
//!
//! - [`PluginRegistry`] — 线程安全的插件注册中心，集成工具自动注册
//! - [`ToolCache`] — 插件工具实例缓存
//! - [`CapabilityChecker`] — 插件能力声明与检查
//! - [`loader::PluginLoader`] — 静态/动态插件加载器
//! - [`sandbox::check_permission`] — 权限检查沙箱

pub mod event;
pub mod hot_reload;
pub mod loader;
pub mod manifest;
pub mod market;
pub mod sandbox;

#[cfg(all(feature = "wasm", not(target_os = "android")))]
pub mod wasm;

pub use event::{Event, EventBus, EventCallback, EventType, Registrar};
pub use hot_reload::{HotReloadEvent, HotReloadWatcher};
pub use manifest::{
    DependencyResolver, ExtendedManifest, MarketMeta, PluginDepType, PluginDependency,
    VersionCompat,
};
pub use market::{
    InstallOptions, MarketPluginEntry, MarketSearchResult, PluginMarket, RegistrySource,
};

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_tool::ToolRegistry;
use lingshu_traits::plugin::{Capability, Plugin, PluginInfo, PluginManifest, PluginStatus};
use tokio::sync::RwLock;
use tracing::info;

// ── ToolCache ───────────────────────────────────────

/// 工具缓存 — 缓存从插件获取的工具实例，避免重复创建.
pub struct ToolCache {
    /// 工具名称 → 工具实例.
    cache: Arc<RwLock<HashMap<String, Box<dyn lingshu_traits::tool::Tool>>>>,
}

impl ToolCache {
    /// 创建空的工具缓存.
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取缓存的工具.
    pub async fn get(&self, name: &str) -> Option<Box<dyn lingshu_traits::tool::Tool>> {
        self.cache.read().await.get(name).map(|t| t.duplicate())
    }

    /// 插入工具到缓存.
    pub async fn insert(&self, name: String, tool: Box<dyn lingshu_traits::tool::Tool>) {
        self.cache.write().await.insert(name, tool);
    }

    /// 批量插入工具.
    pub async fn extend(&self, tools: Vec<(String, Box<dyn lingshu_traits::tool::Tool>)>) {
        let mut cache = self.cache.write().await;
        for (name, tool) in tools {
            cache.insert(name, tool);
        }
    }

    /// 移除缓存的工具.
    pub async fn remove(&self, name: &str) -> Option<Box<dyn lingshu_traits::tool::Tool>> {
        self.cache.write().await.remove(name)
    }

    /// 清空缓存.
    pub async fn clear(&self) {
        self.cache.write().await.clear();
    }

    /// 缓存中的工具数量.
    pub async fn count(&self) -> usize {
        self.cache.read().await.len()
    }

    /// 获取所有缓存的工具名称.
    pub async fn keys(&self) -> Vec<String> {
        self.cache.read().await.keys().cloned().collect()
    }
}

impl Default for ToolCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── CapabilityChecker ───────────────────────────────

/// 能力检查器 — 检查插件是否具备指定能力.
pub struct CapabilityChecker;

impl CapabilityChecker {
    /// 检查插件是否具备指定能力.
    pub fn has_capability(manifest: &PluginManifest, capability: &str) -> bool {
        manifest.capabilities.iter().any(|c| c.name == capability)
    }

    /// 获取插件的所有能力名称.
    pub fn capabilities(manifest: &PluginManifest) -> Vec<String> {
        manifest
            .capabilities
            .iter()
            .map(|c| c.name.clone())
            .collect()
    }

    /// 检查插件是否具备所有指定能力.
    pub fn require_capabilities(manifest: &PluginManifest, required: &[&str]) -> LsResult<()> {
        for cap in required {
            if !Self::has_capability(manifest, cap) {
                return Err(LsError::Plugin(format!(
                    "plugin '{}' missing required capability '{}'",
                    manifest.name, cap
                )));
            }
        }
        Ok(())
    }

    /// 检查两个插件的版本约束是否兼容.
    pub fn check_compat(provider: &Capability, consumer_version: &str) -> LsResult<()> {
        if let Some(ref req_str) = provider.version_req {
            let req = semver::VersionReq::parse(req_str)
                .map_err(|e| LsError::Plugin(format!("invalid version req '{}': {e}", req_str)))?;
            let ver = semver::Version::parse(consumer_version).map_err(|e| {
                LsError::Plugin(format!("invalid version '{}': {e}", consumer_version))
            })?;
            if !req.matches(&ver) {
                return Err(LsError::Plugin(format!(
                    "capability '{}' requires version '{}', consumer is '{}'",
                    provider.name, req_str, consumer_version
                )));
            }
        }
        Ok(())
    }
}

// ── PluginRegistry ──────────────────────────────────

/// 插件注册中心 — 线程安全的插件存储、生命周期管理，集成工具自动注册.
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<LsId, RegistryEntry>>>,
    event_bus: Arc<event::EventBus>,
    /// 可选的 ToolRegistry 引用（用于工具自动注册）.
    tool_registry: Option<Arc<ToolRegistry>>,
    /// 工具缓存.
    tool_cache: Arc<ToolCache>,
}

/// 注册中心中的插件条目.
struct RegistryEntry {
    pub info: PluginInfo,
    pub plugin: Box<dyn Plugin>,
    pub _lib: Option<libloading::Library>,
}

impl PluginRegistry {
    /// 创建一个空的插件注册中心.
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            event_bus: Arc::new(event::EventBus::new()),
            tool_registry: None,
            tool_cache: Arc::new(ToolCache::new()),
        }
    }

    /// 使用指定 EventBus 创建插件注册中心.
    pub fn with_event_bus(event_bus: Arc<event::EventBus>) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            event_bus,
            tool_registry: None,
            tool_cache: Arc::new(ToolCache::new()),
        }
    }

    /// 关联 ToolRegistry — 插件注册时会自动注册声明的工具.
    pub fn with_tool_registry(mut self, tool_registry: Arc<ToolRegistry>) -> Self {
        self.tool_registry = Some(tool_registry);
        self
    }

    /// 获取 EventBus 引用.
    pub fn event_bus(&self) -> &Arc<event::EventBus> {
        &self.event_bus
    }

    /// 获取工具缓存引用.
    pub fn tool_cache(&self) -> &Arc<ToolCache> {
        &self.tool_cache
    }

    /// 获取 ToolRegistry 引用（如果有）.
    pub fn tool_registry(&self) -> Option<&Arc<ToolRegistry>> {
        self.tool_registry.as_ref()
    }

    /// 注册一个插件 (静态或动态)，自动注册工具.
    pub async fn register(
        &self,
        plugin: Box<dyn Plugin>,
        lib: Option<libloading::Library>,
    ) -> LsResult<LsId> {
        let mut info = plugin.info();
        let plugin_id = info.plugin_id;
        let plugin_name = info.manifest.name.clone();

        // ── ToolProvider 工具收集（在插入 map 前进行，避免所有权问题）──
        let provider_tools: Option<Vec<Box<dyn lingshu_traits::tool::Tool>>> =
            if self.tool_registry.is_some() {
                plugin.as_tool_provider().map(|p| p.provided_tools())
            } else {
                None
            };

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

        info!(plugin_id = %plugin_id, name = %plugin_name, "plugin registered");

        // ── ToolProvider 自动注册 ──
        if let Some(ref treg) = self.tool_registry {
            if let Some(tools) = provider_tools {
                if !tools.is_empty() {
                    let tool_names: Vec<String> =
                        tools.iter().map(|t| t.info().name.clone()).collect();
                    info!(
                        plugin = %plugin_name,
                        tools = ?tool_names,
                        "auto-registering plugin tools"
                    );
                    for tool in tools {
                        let tool_name = tool.info().name.clone();
                        // 使用 duplicate() 克隆工具，一份注册到 Registry，一份缓存
                        treg.register(tool.duplicate()).await;
                        self.tool_cache.insert(tool_name, tool).await;
                    }
                }
            }
        }

        self.event_bus
            .publish(&event::Event::new(
                event::EventType::PluginInstalled,
                format!("plugin:{}", plugin_name),
                serde_json::json!({"plugin_id": plugin_id.to_string(), "name": &plugin_name}),
            ))
            .await;
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

    /// 按能力过滤插件.
    pub async fn list_by_capability(&self, capability: &str) -> Vec<PluginInfo> {
        let map = self.plugins.read().await;
        map.values()
            .filter(|e| CapabilityChecker::has_capability(&e.info.manifest, capability))
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
        self.event_bus.publish(
            &event::Event::new(
                event::EventType::PluginLoaded,
                format!("plugin:{}", entry.info.manifest.name),
                serde_json::json!({"plugin_id": plugin_id.to_string(), "name": entry.info.manifest.name}),
            ),
        ).await;
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
        self.event_bus.publish(
            &event::Event::new(
                event::EventType::PluginStarted,
                format!("plugin:{}", entry.info.manifest.name),
                serde_json::json!({"plugin_id": plugin_id.to_string(), "name": entry.info.manifest.name}),
            ),
        ).await;
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
        self.event_bus.publish(
            &event::Event::new(
                event::EventType::PluginStopped,
                format!("plugin:{}", entry.info.manifest.name),
                serde_json::json!({"plugin_id": plugin_id.to_string(), "name": entry.info.manifest.name}),
            ),
        ).await;
        Ok(())
    }

    /// 卸载插件（同时卸载自动注册的工具）.
    pub async fn unregister(&self, plugin_id: &LsId) -> LsResult<()> {
        let mut map = self.plugins.write().await;
        let removed = map
            .remove(plugin_id)
            .ok_or_else(|| LsError::PluginNotFound(plugin_id.to_string()))?;

        // 从 ToolRegistry 注销工具
        if let Some(ref treg) = self.tool_registry {
            for decl in &removed.info.manifest.tools {
                treg.unregister(&decl.name).await;
                self.tool_cache.remove(&decl.name).await;
            }
        }

        info!(plugin_id = %plugin_id, "plugin unregistered");
        self.event_bus.publish(
            &event::Event::new(
                event::EventType::PluginUninstalled,
                format!("plugin:{}", removed.info.manifest.name),
                serde_json::json!({"plugin_id": plugin_id.to_string(), "name": removed.info.manifest.name}),
            ),
        ).await;
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

    /// 检查插件名称是否已注册.
    pub async fn is_name_registered(&self, name: &str) -> bool {
        let map = self.plugins.read().await;
        map.values().any(|e| e.info.manifest.name == name)
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
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

        fn as_any(&self) -> &dyn std::any::Any {
            self
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
            ..PluginManifest::default()
        };
        let info = PluginInfo {
            plugin_id: LsId::new(),
            manifest,
            status: PluginStatus::Installed,
            loaded_at: None,
        };
        Box::new(TestPlugin { info })
    }

    fn make_plugin_with_caps(name: &str, capabilities: Vec<&str>) -> Box<dyn Plugin> {
        let manifest = PluginManifest {
            name: name.into(),
            description: "test plugin".into(),
            plugin_type: "static".into(),
            capabilities: capabilities
                .into_iter()
                .map(|c| Capability {
                    name: c.into(),
                    description: None,
                    version_req: None,
                })
                .collect(),
            ..PluginManifest::default()
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
            description: "dup plugin".into(),
            plugin_type: "static".into(),
            ..PluginManifest::default()
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

    #[tokio::test]
    async fn test_list_by_capability() {
        let registry = PluginRegistry::new();
        registry
            .register(
                make_plugin_with_caps("llm-helper", vec!["llm", "search"]),
                None,
            )
            .await
            .unwrap();
        registry
            .register(make_plugin_with_caps("data-tool", vec!["storage"]), None)
            .await
            .unwrap();

        let llm_plugins = registry.list_by_capability("llm").await;
        assert_eq!(llm_plugins.len(), 1);
        assert_eq!(llm_plugins[0].manifest.name, "llm-helper");

        let storage_plugins = registry.list_by_capability("storage").await;
        assert_eq!(storage_plugins.len(), 1);
        assert_eq!(storage_plugins[0].manifest.name, "data-tool");
    }

    #[tokio::test]
    async fn test_list_by_version() {
        let registry = PluginRegistry::new();
        registry
            .register(make_plugin("v1-plugin"), None)
            .await
            .unwrap();

        // 所有插件版本都是 1.0.0
        let results = registry.list_by_version(">=1.0.0").await;
        assert_eq!(results.len(), 1);

        let no_match = registry.list_by_version(">=2.0.0").await;
        assert!(no_match.is_empty());
    }

    #[tokio::test]
    async fn test_capability_checker() {
        let manifest = PluginManifest {
            name: "test".into(),
            description: "test".into(),
            plugin_type: "static".into(),
            capabilities: vec![
                Capability {
                    name: "llm".into(),
                    description: None,
                    version_req: None,
                },
                Capability {
                    name: "search".into(),
                    description: None,
                    version_req: None,
                },
            ],
            ..PluginManifest::default()
        };

        assert!(CapabilityChecker::has_capability(&manifest, "llm"));
        assert!(!CapabilityChecker::has_capability(&manifest, "storage"));
        assert!(CapabilityChecker::require_capabilities(&manifest, &["llm", "search"]).is_ok());
        assert!(CapabilityChecker::require_capabilities(&manifest, &["llm", "storage"]).is_err());
    }

    #[tokio::test]
    async fn test_tool_cache() {
        let cache = ToolCache::new();
        assert_eq!(cache.count().await, 0);

        use async_trait::async_trait;
        use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};

        struct EchoTool;
        #[async_trait]
        impl Tool for EchoTool {
            fn info(&self) -> ToolInfo {
                ToolInfo::new(
                    "echo",
                    "Echo tool",
                    vec![ToolParam {
                        name: "msg".into(),
                        description: "msg".into(),
                        required: true,
                        param_type: "string".into(),
                    }],
                )
            }
            fn validate(&self, _input: &serde_json::Value) -> LsResult<()> {
                Ok(())
            }
            async fn execute(
                &self,
                _ctx: LsContext,
                input: serde_json::Value,
            ) -> LsResult<serde_json::Value> {
                Ok(input)
            }
            fn duplicate(&self) -> Box<dyn Tool> {
                Box::new(EchoTool)
            }
        }

        cache.insert("echo".into(), Box::new(EchoTool)).await;
        assert_eq!(cache.count().await, 1);
        assert!(cache.keys().await.contains(&"echo".to_string()));

        let cached = cache.get("echo").await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().info().name, "echo");

        cache.remove("echo").await;
        assert_eq!(cache.count().await, 0);
    }
}
