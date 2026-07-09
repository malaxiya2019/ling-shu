//! 🖥️ Lingshu Desktop — Tauri 桌面客户端
//!
//! 将 Lingshu Agent 系统打包为跨平台桌面应用。
//! 支持: Linux / macOS / Windows

use lingshu_core::LsResult;

/// 初始化 Lingshu 运行时并返回 Tauri 构建器.
#[tauri::command]
async fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
async fn get_health() -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "name": "Lingshu Desktop"
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![get_version, get_health])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
