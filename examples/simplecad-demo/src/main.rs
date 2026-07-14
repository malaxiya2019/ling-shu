//! 🖨️ SimpleCAD × Lingshu 集成演示
//!
//! 展示如何使用 SimpleCAD MCP 插件进行 3D CAD 建模。
//!
//! ## 前置条件
//!
//! ```bash
//! pip install simplecadapi mcp
//! # 或
//! uv pip install simplecadapi mcp
//! ```
//!
//! ## 运行
//!
//! ```bash
//! cargo run --example simplecad-demo
//! ```

use lingshu_simplecad_plugin::{init_simplecad, SimpleCadConfig};
use lingshu_core::{LsContext, LsId};
use lingshu_tool::ToolRegistry;
use lingshu_traits::plugin::Plugin;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║   🖨️  SimpleCAD × Lingshu CAD 建模集成演示        ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    // ── 配置 ──────────────────────────────────────────
    let config = SimpleCadConfig {
        python_cmd: "python3".into(),
        server_module: "simplecad_mcp.server".into(),
        tool_timeout_ms: 120_000,
        max_restarts: 2,
        ..Default::default()
    };

    println!("⚙️ 配置:");
    println!("   Python:         {}", config.python_cmd);
    println!("   服务模块:       {}", config.server_module);
    println!("   工具超时:       {}ms", config.tool_timeout_ms);
    println!();

    // ── 一站式初始化 ──────────────────────────────────
    let tool_registry = ToolRegistry::new();

    println!("🚀 启动 SimpleCAD MCP 服务器...");
    let plugin = init_simplecad(&tool_registry, config).await?;

    let status = plugin.plugin_status().await;
    println!("✅ 初始化成功!");
    println!("   名称:           {}", status["name"]);
    println!("   版本:           {}", status["version"]);
    println!("   工具数:         {}", status["discovered_tools"]);
    println!();

    // ── 列出工具 ──────────────────────────────────────
    let tools = plugin.discovered_tools().await;
    println!("🔧 可用 CAD 工具 ({} 个):", tools.len());
    for tool in &tools {
        let info = tool.info();
        println!("   ├── {}", info.name);
    }
    println!();

    // ── 演示：CAD 建模流程 ────────────────────────────
    println!("📐 开始 CAD 建模演示...");
    let ctx = LsContext::with_session(LsId::new());

    // 1. 创建基本体
    let make_box = tools.iter().find(|t| t.info().name == "cad_make_box").unwrap();
    let box_result = make_box.execute(
        ctx.clone(),
        serde_json::json!({
            "dx": 60.0, "dy": 36.0, "dz": 8.0,
            "bottom_face_center": [0.0, 0.0, 0.0],
            "tag": "base_plate",
        }),
    ).await?;
    println!("   📦 创建基板: {}", serde_json::to_string(&box_result).unwrap_or_default());

    // 2. 创建圆柱孔
    let make_cyl = tools.iter().find(|t| t.info().name == "cad_make_cylinder").unwrap();
    let cyl_result = make_cyl.execute(
        ctx.clone(),
        serde_json::json!({
            "radius": 5.0, "height": 14.0,
            "bottom_face_center": [0.0, 0.0, -3.0],
            "tag": "hole",
        }),
    ).await?;
    println!("   🕳️  创建圆柱: {}", serde_json::to_string(&cyl_result).unwrap_or_default());

    // 3. 创建第二个圆柱
    let cyl2_result = make_cyl.execute(
        ctx.clone(),
        serde_json::json!({
            "radius": 8.0, "height": 7.0,
            "bottom_face_center": [-18.0, 0.0, 8.0],
            "tag": "boss",
        }),
    ).await?;
    println!("   🏗️  创建凸台: {}", serde_json::to_string(&cyl2_result).unwrap_or_default());

    // 4. 获取信息
    let get_info = tools.iter().find(|t| t.info().name == "cad_get_info").unwrap();
    let info_result = get_info.execute(
        ctx.clone(),
        serde_json::json!({"shape_tag": "base_plate"}),
    ).await?;
    println!("   ℹ️  基板信息: {}", serde_json::to_string(&info_result).unwrap_or_default());

    println!();
    println!("✅ CAD 建模演示完成!");
    println!();

    // ── 清理 ──────────────────────────────────────────
    println!("🛑 关闭 SimpleCAD...");
    plugin.stop(ctx).await?;
    println!("✅ 已关闭!");

    println!();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║   ✅ SimpleCAD 集成演示完成                       ║");
    println!("║   20+ CAD 建模工具已就绪                         ║");
    println!("╚══════════════════════════════════════════════════╝");

    Ok(())
}
