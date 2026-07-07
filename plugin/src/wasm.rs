//! 🏖️ WASM Sandbox — 基于 WebAssembly 的插件沙箱隔离.
//!
//! 使用 `wasmtime` 运行时在沙箱中执行 WASM 插件，提供更强的安全隔离。
//! 支持组件模型（Component Model）和 WASI 接口。
//!
//! **注意**: 此模块需要 `wasm` feature 启用。

use lingshu_core::{LsError, LsResult};
use lingshu_traits::plugin::{PluginInfo, PluginManifest, PluginStatus};
use std::path::Path;
use tracing::info;

/// WASM 沙箱配置.
#[derive(Debug, Clone)]
pub struct WasmSandboxConfig {
    /// 最大内存 (字节).
    pub max_memory: u64,
    /// 是否启用网络访问.
    pub enable_network: bool,
    /// 是否启用文件系统访问.
    pub enable_filesystem: bool,
    /// 允许访问的目录列表.
    pub allowed_dirs: Vec<String>,
    /// 引擎版本.
    pub engine_version: String,
}

impl Default for WasmSandboxConfig {
    fn default() -> Self {
        Self {
            max_memory: 64 * 1024 * 1024, // 64 MB
            enable_network: false,
            enable_filesystem: false,
            allowed_dirs: vec![],
            engine_version: "1.0.0".into(),
        }
    }
}

/// WASM 插件沙箱.
pub struct WasmSandbox {
    /// 运行时引擎.
    engine: Option<wasmtime::Engine>,
    /// 沙箱配置.
    config: WasmSandboxConfig,
    /// 已加载的 WASM 模块.
    modules: Vec<(PluginInfo, wasmtime::Store<Option<wasmtime_wasi::WasiCtx>>)>,
}

impl WasmSandbox {
    /// 创建新的 WASM 沙箱.
    pub fn new(config: WasmSandboxConfig) -> Self {
        Self {
            engine: None,
            config,
            modules: Vec::new(),
        }
    }

    /// 初始化 WASM 运行时.
    pub fn init(&mut self) -> LsResult<()> {
        let mut wasm_config = wasmtime::Config::new();
        wasm_config.wasm_component_model(true);
        wasm_config.wasm_multi_value(true);
        wasm_config.wasm_memory64(true);

        // 设置内存限制
        wasm_config.max_memory_size(self.config.max_memory);

        let engine = wasmtime::Engine::new(&wasm_config)
            .map_err(|e| LsError::Plugin(format!("failed to create WASM engine: {e}")))?;

        self.engine = Some(engine);
        info!("WASM sandbox initialized");
        Ok(())
    }

    /// 从 WASM 文件加载插件.
    pub fn load_plugin(&mut self, path: &Path, manifest: &PluginManifest) -> LsResult<PluginInfo> {
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| LsError::Plugin("WASM sandbox not initialized".into()))?;

        let wasm_bytes = std::fs::read(path).map_err(|e| {
            LsError::Plugin(format!("cannot read WASM file '{}': {e}", path.display()))
        })?;

        // 创建模块
        let module = wasmtime::Module::new(engine, &wasm_bytes)
            .map_err(|e| LsError::Plugin(format!("invalid WASM module: {e}")))?;

        // 创建 store (用于沙箱状态跟踪)
        let mut store = wasmtime::Store::new(engine, None::<wasmtime_wasi::WasiCtx>);

        // 创建链接器 (限制 WASI 访问)
        let mut linker = wasmtime::Linker::new(engine);

        // 根据配置选择性添加 WASI
        if self.config.enable_filesystem {
            let wasi_ctx_builder = wasmtime_wasi::WasiCtxBuilder::new()
                .inherit_stderr()
                .inherit_stdout();

            let mut wasi_ctx_builder = wasi_ctx_builder;
            for dir in &self.config.allowed_dirs {
                wasi_ctx_builder = wasi_ctx_builder
                    .preopened_dir(dir, dir)
                    .map_err(|e| {
                        LsError::Plugin(format!("cannot preopen dir '{}': {e}", dir))
                    })?;
            }

            let wasi = wasi_ctx_builder.build();
            *store.data_mut() = Some(wasi);

            wasmtime_wasi::add_to_linker(&mut linker, |state| state.as_mut().unwrap())
                .map_err(|e| LsError::Plugin(format!("failed to add WASI linker: {e}")))?;
        }

        // 实例化模块
        let _instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| LsError::Plugin(format!("failed to instantiate WASM module: {e}")))?;

        let plugin_id = lingshu_core::LsId::new();
        let info = PluginInfo {
            plugin_id,
            manifest: manifest.clone(),
            status: PluginStatus::Loaded,
            loaded_at: Some(chrono::Utc::now()),
        };

        self.modules.push((info.clone(), store));
        info!(
            name = %manifest.name,
            version = %manifest.version,
            "WASM plugin loaded"
        );

        Ok(info)
    }

    /// 在 WASM 沙箱中执行一个插件函数.
    pub fn invoke(
        &self,
        _plugin_id: &lingshu_core::LsId,
        _function: &str,
        _args: &[u8],
    ) -> LsResult<Vec<u8>> {
        // 实际调用 WASM 模块的导出函数
        // 由于编译器差异，此处为占位实现
        Err(LsError::Plugin(
            "WASM plugin invocation not yet implemented".into(),
        ))
    }

    /// 卸载 WASM 插件.
    pub fn unload(&mut self, plugin_id: &lingshu_core::LsId) -> LsResult<()> {
        let pos = self
            .modules
            .iter()
            .position(|(info, _)| info.plugin_id == *plugin_id)
            .ok_or_else(|| LsError::PluginNotFound(plugin_id.to_string()))?;

        self.modules.remove(pos);
        info!(plugin_id = %plugin_id, "WASM plugin unloaded");
        Ok(())
    }

    /// 获取已加载的 WASM 插件列表.
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.modules.iter().map(|(info, _)| info.clone()).collect()
    }

    /// 检查 WASM 沙箱是否可用.
    pub fn is_available(&self) -> bool {
        self.engine.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default() {
        let config = WasmSandboxConfig::default();
        assert_eq!(config.max_memory, 64 * 1024 * 1024);
        assert!(!config.enable_network);
        assert!(!config.enable_filesystem);
    }

    #[test]
    fn test_sandbox_init() {
        let mut sandbox = WasmSandbox::new(WasmSandboxConfig::default());
        let _ = sandbox.init();
    }

    #[test]
    fn test_sandbox_not_available_by_default() {
        let sandbox = WasmSandbox::new(WasmSandboxConfig::default());
        assert!(!sandbox.is_available());
    }

    #[test]
    fn test_list_empty() {
        let sandbox = WasmSandbox::new(WasmSandboxConfig::default());
        assert!(sandbox.list_plugins().is_empty());
    }
}
