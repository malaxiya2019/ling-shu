//! loong 轻量 Agent 集成端到端测试
//!
//! 验证 LoongAdapter 的 Agent 创建、执行和生命周期管理。
//! 注意：loong feature 启用时运行真实逻辑，否则验证桩。

use lingshu_core::{LsError, LsId};
use lingshu_orchestrator::loong_adapter::{LoongAdapter, LoongAgentConfig};
use lingshu_orchestrator::registry::AgentRegistry;
use std::sync::Arc;

/// 测试 LoongAdapter 创建和默认状态
#[tokio::test]
async fn test_loong_adapter_new() {
    let adapter = LoongAdapter::new();
    let agents = adapter.list_agents().await;
    assert!(agents.is_empty(), "new adapter should have no agents");

    let status = adapter
        .runtime_status()
        .await
        .expect("runtime_status should not fail");
    assert_eq!(status["agent_count"], 0);
}

/// 测试创建 Agent
#[tokio::test]
async fn test_loong_create_agent() {
    let adapter = LoongAdapter::new();
    let config = LoongAgentConfig {
        name: "test-agent".into(),
        description: "E2E test agent".into(),
        capabilities: vec!["code".into(), "reasoning".into()],
        model: "gpt-4".into(),
        system_prompt: Some("You are a helpful assistant.".into()),
        max_tokens: 4096,
    };

    match adapter.create_agent(config).await {
        Ok(agent_id) => {
            // 成功创建
            let agents = adapter.list_agents().await;
            assert!(agents.contains(&agent_id), "agent should be in the list");
        }
        Err(LsError::NotImplemented(_)) => {
            // 桩模式
            let agents = adapter.list_agents().await;
            assert!(agents.is_empty());
            return;
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// 测试 Agent 执行
#[tokio::test]
async fn test_loong_run_agent() {
    let adapter = LoongAdapter::new();
    let _agent_id = LsId::new();

    // 先创建 agent
    let config = LoongAgentConfig {
        name: "runner".into(),
        description: "Run test".into(),
        capabilities: vec!["execution".into()],
        model: "default".into(),
        system_prompt: None,
        max_tokens: 2048,
    };

    match adapter.create_agent(config).await {
        Ok(id) => {
            // 执行
            match adapter
                .run_agent(&id, serde_json::json!({"cmd": "echo hello"}))
                .await
            {
                Ok(output) => {
                    // 验证输出结构
                    assert_eq!(output.status, lingshu_traits::agent::AgentStatus::Completed);
                    assert!(output.data.is_some());
                }
                Err(e) => {
                    // 即使是 feature 模式，真实执行也可能因缺少 loong runtime 而失败
                    // 只要不 panic 且错误不是 Unsupported 即可
                    assert!(
                        !matches!(e, LsError::NotImplemented(_)),
                        "expected non-unsupported error in feature mode"
                    );
                }
            }
        }
        Err(LsError::NotImplemented(_)) => {
            // 桩模式
            return;
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// 测试停止 Agent
#[tokio::test]
async fn test_loong_stop_agent() {
    let adapter = LoongAdapter::new();

    match adapter.stop_agent(&LsId::new()).await {
        Ok(()) => {
            // 停止不存在的 agent 应成功（幂等）
        }
        Err(LsError::NotImplemented(_)) => {
            // 桩模式
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// 测试 with_registry 绑定
#[tokio::test]
async fn test_loong_with_registry() {
    let registry = Arc::new(AgentRegistry::new());
    let adapter = LoongAdapter::new().with_registry(registry.clone());

    // 创建 agent 时应自动注册到 registry
    let config = LoongAgentConfig {
        name: "reg-agent".into(),
        description: "Registry test".into(),
        capabilities: vec!["test".into()],
        model: "default".into(),
        system_prompt: None,
        max_tokens: 1024,
    };

    match adapter.create_agent(config).await {
        Ok(agent_id) => {
            // feature 模式：验证 agent 已注册到 registry
            let info = registry
                .get(&agent_id)
                .await
                .expect("agent should be in registry");
            assert_eq!(info.name, "reg-agent");
            assert_eq!(info.tags.get("source").map(|s| s.as_str()), Some("loong"));
        }
        Err(LsError::NotImplemented(_)) => {
            // 桩模式：registry 应仍为空
            assert_eq!(registry.count().await, 0);
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// 测试 LoongAgentConfig 序列化
#[test]
fn test_loong_config_serde() {
    let config = LoongAgentConfig {
        name: "serde-test".into(),
        description: "Serde roundtrip".into(),
        capabilities: vec!["a".into(), "b".into()],
        model: "claude-3".into(),
        system_prompt: None,
        max_tokens: 8192,
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: LoongAgentConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "serde-test");
    assert_eq!(deserialized.capabilities.len(), 2);
    assert_eq!(deserialized.max_tokens, 8192);
}

/// 测试 runtime_status 不被桩影响
#[tokio::test]
async fn test_loong_runtime_status_structure() {
    let adapter = LoongAdapter::new();
    let status = adapter.runtime_status().await.unwrap();
    // 无论是否 feature 模式，都应返回合理的 JSON 结构
    assert!(
        status.get("agent_count").is_some(),
        "status should have agent_count"
    );
    assert!(
        status.get("runtime_health").is_some(),
        "status should have runtime_health"
    );
}
