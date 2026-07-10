//! Plugin Loader — 插件清单加载与动态/静态插件装载.

use std::path::Path;

use lingshu_core::{LsError, LsResult};
use lingshu_traits::plugin::{Plugin, PluginInfo, PluginManifest, PluginStatus};
use tracing::info;

use crate::sandbox::check_api_version;

/// 插件加载器 — 负责加载 manifest、静态插件与动态插件.
pub struct PluginLoader {
    /// 当前运行的 API 版本字符串 (用于兼容性检查).
    api_version: String,
}

impl PluginLoader {
    /// 创建新的插件加载器.
    pub fn new(api_version: &str) -> Self {
        Self {
            api_version: api_version.to_string(),
        }
    }

    /// 从 JSON 文件加载插件清单.
    pub fn load_manifest(path: &Path) -> LsResult<PluginManifest> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            LsError::Plugin(format!("cannot read manifest '{}': {e}", path.display()))
        })?;
        let manifest: PluginManifest = serde_json::from_str(&content)
            .map_err(|e| LsError::Plugin(format!("invalid manifest '{}': {e}", path.display())))?;
        info!(name = %manifest.name, version = %manifest.version, "plugin manifest loaded");
        Ok(manifest)
    }

    /// 加载静态插件 (直接在进程内注册).
    pub fn load_static(&self, plugin: Box<dyn Plugin>) -> LsResult<PluginInfo> {
        let info = plugin.info();
        let manifest = &info.manifest;
        check_api_version(manifest, &self.api_version)?;
        info!(name = %manifest.name, "static plugin loaded");
        Ok(PluginInfo {
            status: PluginStatus::Loaded,
            ..info
        })
    }

    /// 加载动态插件 (从 .so / .dylib 文件).
    ///
    /// 使用 `libloading` 在运行时加载共享库，并调用 `create_plugin` 符号
    /// 获取 `Box<dyn Plugin>` 实例.
    ///
    /// # Safety
    ///
    /// 调用者必须确保 `.so` / `.dylib` 文件来源可信，且与宿主 ABI 兼容。
    /// 加载不受信任的动态库可能导致代码注入或进程崩溃。
    pub unsafe fn load_dynamic(&self, path: &Path) -> LsResult<(PluginInfo, libloading::Library)> {
        let lib = libloading::Library::new(path).map_err(|e| {
            LsError::Plugin(format!("cannot load library '{}': {e}", path.display()))
        })?;

        // 尝试从共享库中获取 create_plugin 函数
        let create: libloading::Symbol<unsafe extern "C" fn() -> Box<dyn Plugin>> = {
            let symbol: Result<libloading::Symbol<unsafe extern "C" fn() -> Box<dyn Plugin>>, _> =
                lib.get(b"create_plugin");
            match symbol {
                Ok(s) => s,
                Err(e) => {
                    return Err(LsError::Plugin(format!(
                        "symbol 'create_plugin' not found in '{}': {e}",
                        path.display()
                    )));
                }
            }
        };

        let plugin = create();
        let mut info = plugin.info();
        check_api_version(&info.manifest, &self.api_version)?;
        info.status = PluginStatus::Loaded;
        info!(
            name = %info.manifest.name,
            version = %info.manifest.version,
            "dynamic plugin loaded from {}",
            path.display()
        );
        Ok((info, lib))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_core::{LsContext, LsId};
    use lingshu_traits::plugin::PluginPermission;

    struct MockPlugin {
        info: PluginInfo,
    }

    #[async_trait]
    impl Plugin for MockPlugin {
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

    #[test]
    fn test_load_static() {
        let loader = PluginLoader::new("1.0.0");
        let manifest = PluginManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            description: "test".into(),
            author: None,
            homepage: None,
            license: None,
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![],
            min_api_version: Some("0.9.0".into()),
        ..Default::default()
        };
        let info = PluginInfo {
            plugin_id: LsId::new(),
            manifest,
            status: PluginStatus::Installed,
            loaded_at: None,
        };
        let plugin = MockPlugin { info };
        let result = loader.load_static(Box::new(plugin));
        assert!(result.is_ok());
        let loaded = result.unwrap();
        assert_eq!(loaded.status, PluginStatus::Loaded);
    }

    #[test]
    fn test_load_manifest_invalid_path() {
        let result = PluginLoader::load_manifest(Path::new("/nonexistent/plugin.json"));
        assert!(result.is_err());
    }
}
