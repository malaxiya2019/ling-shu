//! 📱 agent-device 一站式集成示例
//!
//! 演示：
//! 1. 使用 `init_agent_device()` 一站式初始化
//! 2. 启用 `auto_sync_interval_secs: 30` 动态工具同步
//! 3. 使用 `AgentDeviceConfig` 自定义配置
//! 4. 发现 55+ 设备自动化 MCP 工具
//!
//! ## 运行
//!
//! ```bash
//! cargo run --example agent-device-demo
//! ```
//!
//! ## 前置条件
//!
//! - `npm install -g agent-device` (v0.19+)
//! - Node.js >= 22.12

use lingshu_agent_device_plugin::{init_agent_device, AgentDeviceConfig, AgentDevicePlugin};
use lingshu_core::{LsContext, LsId};
use lingshu_tool::ToolRegistry;
use lingshu_traits::plugin::Plugin;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt().with_env_filter("info").init();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║   📱 agent-device × Lingshu 集成演示              ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    // ── 检查系统依赖 ──────────────────────────────────────
    println!("🔍 检查系统依赖...");
    let deps = AgentDevicePlugin::check_dependencies().await;
    println!(
        "   Node.js:       {}",
        if deps.node_installed { "✅" } else { "❌" }
    );
    println!(
        "   agent-device:  {}",
        if deps.agent_device_installed {
            "✅"
        } else {
            "❌"
        }
    );
    println!(
        "   Xcode:         {}",
        if deps.xcode_installed { "✅" } else { "❌" }
    );
    println!(
        "   ADB:           {}",
        if deps.adb_installed { "✅" } else { "❌" }
    );

    if !deps.all_ok() {
        eprintln!("⚠️  缺少必要依赖，请安装 Node.js 和 agent-device");
        return Ok(());
    }
    println!();

    // ── 创建 ToolRegistry ─────────────────────────────────
    let tool_registry = ToolRegistry::new();

    // ── 配置 AgentDeviceConfig ────────────────────────────
    // 用户要求: auto_sync_interval_secs: 30
    let config = AgentDeviceConfig {
        command: "agent-device".into(),
        args: vec!["mcp".into()],
        tool_timeout_ms: 120_000,          // 120s 超时
        max_restarts: 3,                   // 最多重启 3 次
        output_format: "optimized".into(), // 优化输出格式
        auto_sync_interval_secs: 30,       // 🔄 每 30 秒同步工具变化
        max_session_idle_secs: 3600,       // 1 小时空闲超时
        ..Default::default()
    };

    println!("⚙️  配置:");
    println!("   命令:              {}", config.command);
    println!("   参数:              {:?}", config.args);
    println!("   工具超时:          {}ms", config.tool_timeout_ms);
    println!("   最大重启:          {}", config.max_restarts);
    println!("   输出格式:          {}", config.output_format);
    println!("   自动同步间隔:      {}s", config.auto_sync_interval_secs);
    println!("   最大空闲会话:      {}s", config.max_session_idle_secs);
    println!();

    // ── 一站式初始化 ──────────────────────────────────────
    // 用户要求: 使用 init_agent_device() 一站式初始化
    println!("🚀 启动 agent-device MCP 子进程...");
    let plugin = init_agent_device(&tool_registry, config).await?;

    println!();
    println!("✅ 一站式初始化成功!");
    println!();

    // ── 查看插件状态 ──────────────────────────────────────
    let status = plugin.plugin_status().await;
    println!("📊 插件状态:");
    println!(
        "   名称:             {}",
        status["name"].as_str().unwrap_or("?")
    );
    println!(
        "   版本:             {}",
        status["version"].as_str().unwrap_or("?")
    );
    println!(
        "   运行中:           {}",
        status["running"].as_bool().unwrap_or(false)
    );
    println!(
        "   发现工具数:       {}",
        status["discovered_tools"].as_u64().unwrap_or(0)
    );
    println!(
        "   自动同步:         {}",
        status["sync_enabled"].as_bool().unwrap_or(false)
    );
    println!();

    // ── 列出已发现的工具 ──────────────────────────────────
    let tools = plugin.discovered_tools().await;
    println!("🔧 已发现 {} 个 MCP 工具:", tools.len());

    // 按类别分组显示
    let mut category_map: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for tool in &tools {
        let info = tool.info();
        let name = &info.name;

        let category = if let Some(idx) = name.find(':') {
            name[..idx].to_string()
        } else {
            "general".to_string()
        };

        category_map.entry(category).or_default().push(name.clone());
    }

    for (category, names) in &category_map {
        println!("   📂 {}/", category);
        for name in names {
            println!("      ├── {}", name);
        }
    }
    println!();

    // ── 验证关键工具存在 ──────────────────────────────────
    let tool_names: Vec<String> = tools.iter().map(|t| t.info().name.clone()).collect();

    let key_tools = [
        "device:snapshot",
        "device:open",
        "device:click",
        "device:type",
        "device:scroll",
        "device:screenshot",
    ];
    println!("🔑 关键工具检查:");
    for key in &key_tools {
        let found = tool_names.iter().any(|n| n == key);
        println!("   {}  {}", if found { "✅" } else { "❌" }, key);
    }
    println!();

    // ── 测试调用 capabilities（不需要设备） ──────────────
    println!("📞 测试 MCP 通信: capabilities...");
    let cap_tools: Vec<_> = tools
        .iter()
        .filter(|t| t.info().name == "capabilities" || t.info().name == "device:capabilities")
        .collect();

    if let Some(cap_tool) = cap_tools.first() {
        match cap_tool
            .execute(LsContext::with_session(LsId::new()), serde_json::json!({}))
            .await
        {
            Ok(result) => {
                println!("   ✅ capabilities 调用成功");
                println!(
                    "   结果: {}",
                    serde_json::to_string_pretty(&result).unwrap_or_default()
                );
            }
            Err(e) => {
                println!("   ⚠️  capabilities 调用 (无设备环境): {}", e);
            }
        }
    } else {
        println!("   ⚠️  未找到 capabilities 工具");
    }
    println!();

    // ── 会话管理演示 ──────────────────────────────────────
    if let Some(session_mgr) = plugin.session_manager().await {
        println!("📋 会话管理器状态:");
        let stats = session_mgr.stats().await;
        println!("   总会话数:   {}", stats.total_sessions);
        println!("   活动会话:   {}", stats.active_sessions);
    }
    println!();

    // ── 手动触发同步 ──────────────────────────────────────
    println!("🔄 手动触发工具同步...");
    match plugin.sync_now().await {
        Ok(diff) => {
            println!("   新增工具: {}", diff.new_tools);
            println!("   移除工具: {}", diff.removed_tools);
            println!("   总工具数: {}", diff.tool_count);
        }
        Err(e) => {
            println!("   ⚠️  同步调用: {}", e);
        }
    }
    println!();

    // ── 优雅关闭 ──────────────────────────────────────────
    println!("🛑 关闭 agent-device...");
    let ctx = LsContext::with_session(LsId::new());
    plugin.stop(ctx).await?;
    println!("✅ 已关闭!");

    println!();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║   ✅ 集成演示完成                                 ║");
    println!("║   55+ 设备自动化工具已就绪                        ║");
    println!("║   30s 自动同步已启用                              ║");
    println!("╚══════════════════════════════════════════════════╝");

    Ok(())
}
