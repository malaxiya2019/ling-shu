//! agent-device 插件集成测试
//!
//! 测试 MCP stdio 子进程的启动、工具发现和基本工具调用。
//! 需要系统已安装 agent-device (`npm install -g agent-device`)。



use lingshu_mcp::rmcp_stdio_client::{McpStdioClient, McpStdioConfig};

/// 测试直接通过 McpStdioClient 启动 agent-device MCP 子进程并列出工具。
#[tokio::test]
async fn test_mcp_stdio_client_discovers_tools() {
    let config = McpStdioConfig {
        command: "agent-device".into(),
        args: vec!["mcp".into()],
        spawn_timeout_ms: 15_000,
        tool_timeout_ms: 30_000,
        ..Default::default()
    };

    let client = McpStdioClient::spawn(config)
        .await
        .expect("Failed to spawn agent-device MCP process");

    // 等待 MCP 子进程就绪
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    // 健康检查
    client
        .health_check()
        .await
        .expect("MCP health check failed");

    // 列出工具
    let tools = client
        .list_tools()
        .await
        .expect("Failed to list MCP tools");

    assert!(!tools.is_empty(), "Should discover at least one MCP tool");
    assert!(
        tools.iter().any(|t| t.name == "snapshot"),
        "Should discover 'snapshot' tool"
    );
    assert!(
        tools.iter().any(|t| t.name == "open"),
        "Should discover 'open' tool"
    );
    assert!(
        tools.iter().any(|t| t.name == "click" || t.name == "tap"),
        "Should discover 'click' tool"
    );

    println!("✅ Discovered {} MCP tools", tools.len());
    for tool in &tools {
        println!("  - {}: {}", tool.name, &tool.description[..tool.description.len().min(60)]);
    }

    // 测试 tools/call 对 version 命令（不需要设备）
    let result = client
        .call_tool("capabilities", serde_json::json!({}))
        .await;

    match result {
        Ok(result) => {
            println!("✅ tools/call capabilities succeeded");
            assert!(!result.is_error, "capabilities should not error");
        }
        Err(e) => {
            // 在没有设备的环境中，capabilities 可能会失败，这不影响测试
            println!("ℹ️  capabilities call (expected without device): {e}");
        }
    }

    // 关闭
    client
        .shutdown()
        .await
        .expect("Failed to shutdown MCP process");

    println!("✅ MCP stdio client integration test passed");
}

/// 测试 McpStdioClient 的重启功能。
#[tokio::test]
async fn test_mcp_stdio_client_restart() {
    let config = McpStdioConfig {
        command: "agent-device".into(),
        args: vec!["mcp".into()],
        spawn_timeout_ms: 15_000,
        tool_timeout_ms: 30_000,
        max_restarts: 2,
        ..Default::default()
    };

    let client = McpStdioClient::spawn(config)
        .await
        .expect("Failed to spawn MCP process");

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    // 首次健康检查
    client.health_check().await.expect("Initial health check failed");

    // 重启
    client.restart().await.expect("First restart should succeed");

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    // 重启后健康检查
    client
        .health_check()
        .await
        .expect("Health check after restart failed");

    // 重启后应仍能列出工具
    let tools = client.list_tools().await.expect("list_tools after restart failed");
    assert!(!tools.is_empty(), "Should discover tools after restart");

    println!("✅ Restart test passed: discovered {} tools after restart", tools.len());

    client.shutdown().await.expect("Shutdown failed");
}

/// 测试通过 AgentDevicePlugin 的高层集成。
/// 此测试使用完整的 Plugin API。
#[tokio::test]
async fn test_agent_device_plugin_integration() {
    use lingshu_agent_device_plugin::AgentDevicePlugin;
    use lingshu_traits::plugin::Plugin;
    use lingshu_core::LsContext;

    let plugin = AgentDevicePlugin::new("agent-device".into());
    let ctx = LsContext::with_session(lingshu_core::LsId::new());

    // Init
    plugin
        .init(ctx.clone())
        .await
        .expect("Plugin init should succeed");

    // Start + discover
    let tools = plugin
        .start_and_discover(&ctx)
        .await
        .expect("Plugin start_and_discover should succeed");

    assert!(!tools.is_empty(), "Should discover tools via Plugin API");
    println!(
        "✅ Discovered {} tools via AgentDevicePlugin",
        tools.len()
    );

    // 验证一些关键工具存在
    let tool_names: Vec<String> = tools.iter().map(|t| t.info().name).collect();
    assert!(
        tool_names.iter().any(|n| n == "device:snapshot"),
        "Should have device:snapshot"
    );
    assert!(
        tool_names.iter().any(|n| n == "device:open"),
        "Should have device:open"
    );
    assert!(
        tool_names.iter().any(|n| n == "device:click"),
        "Should have device:click"
    );

    // 插件状态应该显示运行中
    let status = plugin.plugin_status().await;
    assert!(status["running"].as_bool().unwrap_or(false));
    assert_eq!(status["discovered_tools"].as_u64().unwrap_or(0) as usize, tools.len());

    println!("✅ Plugin status: running={}, tools={}", 
        status["running"], status["discovered_tools"]);

    // Stop
    plugin.stop(ctx).await.expect("Plugin stop should succeed");
    println!("✅ Plugin integration test passed");
}
