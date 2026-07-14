//! Hello Plugin — Lingshu 插件示例.
//!
//! 展示如何创建一个动态加载的 Lingshu 插件。
//! 编译为 `.so`/`.dylib` 后可通过 PluginLoader::load_dynamic 加载。

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::plugin::{
    Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus,
};

/// Hello 插件结构.
struct HelloPlugin {
    info: PluginInfo,
}

#[async_trait]
impl Plugin for HelloPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        println!("[hello-plugin] init");
        Ok(())
    }

    async fn start(&self, ctx: LsContext) -> LsResult<()> {
        println!("[hello-plugin] Hello from Lingshu plugin! (session: {})", ctx.session_id);
        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        println!("[hello-plugin] stop");
        Ok(())
    }

        fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn required_permissions(&self) -> Vec<PluginPermission> {
        self.info.manifest.permissions.clone()
    }
}

/// 插件创建函数 — 动态库入口.
///
/// 动态插件必须导出此函数，返回 `Box<dyn Plugin>`.
#[no_mangle]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    let manifest = PluginManifest {
        name: "hello-plugin".into(),
        version: "1.0.0".into(),
        description: "A simple Hello World plugin".into(),
        author: Some("Lingshu Team".into()),
        homepage: Some("https://github.com/lingshu-org/hello-plugin".into()),
        license: Some("MIT".into()),
        plugin_type: "dynamic".into(),
        entry_point: Some("create_plugin".into()),
        permissions: vec![PluginPermission {
            resource: "logging".into(),
            actions: vec!["info".into(), "warn".into()],
        }],
        min_api_version: Some("1.0.0".into()),
    ..Default::default()
    };

    let info = PluginInfo {
        plugin_id: LsId::new(),
        manifest,
        status: PluginStatus::Installed,
        loaded_at: None,
    };

    Box::new(HelloPlugin { info })
}
