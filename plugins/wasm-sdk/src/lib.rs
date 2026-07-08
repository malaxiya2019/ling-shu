//! LingShu WASM Plugin SDK
//!
//! 此 crate 是 WASM 插件模板。编译为目标 `wasm32-wasip1`，
//! 生成 `.wasm` 文件供 lingshu-plugin-wasmtime 加载执行。

use serde::{Deserialize, Serialize};

/// 插件元数据。
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
}

/// 插件输入。
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginInput {
    pub method: String,
    pub params: serde_json::Value,
}

/// 插件输出。
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginOutput {
    pub success: bool,
    pub data: serde_json::Value,
    pub error: Option<String>,
}

/// 返回插件元信息。
#[no_mangle]
pub extern "C" fn plugin_info() -> *mut u8 {
    let info = PluginInfo {
        name: "lingshu-wasm-plugin",
        version: "0.1.0",
        description: "LingShu WASM Plugin Template",
    };
    let json = serde_json::to_string(&info).unwrap();
    let bytes = json.into_bytes();
    let ptr = bytes.as_ptr() as *mut u8;
    std::mem::forget(bytes);
    ptr
}

/// 核心处理入口。
#[no_mangle]
pub extern "C" fn plugin_exec(input_ptr: *const u8, input_len: usize) -> *mut u8 {
    let input_bytes = unsafe { std::slice::from_raw_parts(input_ptr, input_len) };
    let input: PluginInput = match serde_json::from_slice(input_bytes) {
        Ok(v) => v,
        Err(e) => {
            let output = PluginOutput {
                success: false,
                data: serde_json::Value::Null,
                error: Some(format!("parse error: {e}")),
            };
            let json = serde_json::to_string(&output).unwrap();
            let bytes = json.into_bytes();
            let ptr = bytes.as_ptr() as *mut u8;
            std::mem::forget(bytes);
            return ptr;
        }
    };

    let result = format!("handled method: {} with params: {}", input.method, input.params);
    let output = PluginOutput {
        success: true,
        data: serde_json::json!({ "result": result }),
        error: None,
    };

    let json = serde_json::to_string(&output).unwrap();
    let bytes = json.into_bytes();
    let ptr = bytes.as_ptr() as *mut u8;
    std::mem::forget(bytes);
    ptr
}

/// 释放由插件分配的内存。
#[no_mangle]
pub extern "C" fn plugin_free(ptr: *mut u8, len: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, len, len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_info() {
        let ptr = plugin_info();
        let info_bytes = unsafe { std::ffi::CStr::from_ptr(ptr as *const i8) }
            .to_str()
            .unwrap()
            .to_string();
        let info: PluginInfo = serde_json::from_str(&info_bytes).unwrap();
        assert_eq!(info.name, "lingshu-wasm-plugin");
    }

    #[test]
    fn test_plugin_exec() {
        let input = PluginInput {
            method: "greet".into(),
            params: serde_json::json!({ "name": "world" }),
        };
        let json = serde_json::to_string(&input).unwrap();
        let bytes = json.as_bytes();
        let ptr = plugin_exec(bytes.as_ptr(), bytes.len());

        let output_bytes = unsafe { std::slice::from_raw_parts(ptr, 1024) };
        let len = output_bytes.iter().position(|&b| b == 0).unwrap_or(output_bytes.len());
        let out_str = std::str::from_utf8(&output_bytes[..len]).unwrap();
        let output: PluginOutput = serde_json::from_str(out_str).unwrap();
        assert!(output.success);
    }
}
