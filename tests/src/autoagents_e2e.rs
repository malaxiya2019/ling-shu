//! AutoAgents 集成端到端测试
//!
//! 验证 AutoAgentsOrchestrator 的编队创建、Agent 注册和任务委派。
//! 注意：autoagents feature 启用时运行真实 ReAct 逻辑，
//! 否则验证桩模块返回正确的 Unsupported 错误。

use lingshu_core::{LsContext, LsError, LsId};
use lingshu_orchestrator::autoagents_bridge::{
    AutoAgentsOrchestrator, CrewConfig, ReActConfig,
};
use lingshu_orchestrator::orchestrator::OrchestratorConfig;

/// 测试编队 (Crew) 的创建和查询
#[tokio::test]
async fn test_autoagents_create_crew() {
    let orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());

    let config = CrewConfig {
        name: "test-crew".into(),
        description: "E2E test crew".into(),
        required_capabilities: vec!["code".into(), "reasoning".into()],
        react_config: ReActConfig::default(),
        parallel_execution: true,
    };

    match orch.create_crew(config.clone()).await {
        Ok(()) => {
            // 成功创建（feature 模式）
            let crews = orch.list_crews().await;
            assert_eq!(crews.len(), 1);
            assert_eq!(crews[0].name, "test-crew");
        }
        Err(LsError::NotImplemented(_)) => {
            // 桩模式：验证编队列表为空
            let crews = orch.list_crews().await;
            assert!(crews.is_empty());
            return;
        }
        Err(e) => panic!("unexpected error: {e}"),
    }

    // 重复创建应失败
    let dup = CrewConfig {
        name: "test-crew".into(),
        description: "Duplicate crew".into(),
        ..config
    };
    let err = orch.create_crew(dup).await;
    assert!(err.is_err(), "duplicate crew creation should fail");
}

/// 测试 Agent 注册
#[tokio::test]
async fn test_autoagents_register_agent() {
    let orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());
    let agent_id = LsId::new();

    match orch
        .register_agent(agent_id, "react-agent", vec!["code", "reasoning"])
        .await
    {
        Ok(()) => {
            // 成功注册（feature 模式）
            // 注册成功即验证通过
            let _ = agent_id;
        }
        Err(LsError::NotImplemented(_)) => {
            // 桩模式
            return;
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// 测试任务委派（回退到标准编排）
#[tokio::test]
async fn test_autoagents_delegate_react() {
    let orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());
    let ctx = LsContext::with_session(LsId::new());

    // 先创建编队
    let config = CrewConfig {
        name: "workers".into(),
        description: "Worker crew".into(),
        required_capabilities: vec!["work".into()],
        react_config: ReActConfig::default(),
        parallel_execution: false,
    };
    let _ = orch.create_crew(config).await;

    match orch
        .delegate_react("workers", serde_json::json!({"task": "do work"}), &ctx)
        .await
    {
        Ok(result) => {
            // 成功委派
            assert_eq!(result.team, "workers");
        }
        Err(LsError::NotImplemented(_)) => {
            // 桩模式
        }
        Err(LsError::NotFound(_)) => {
            // 桩模式下 crew 可能不存在
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// 测试不存在的编队委派应返回 NotFound
#[tokio::test]
async fn test_autoagents_delegate_nonexistent_crew() {
    let orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());
    let ctx = LsContext::with_session(LsId::new());

    let err = orch
        .delegate_react("nonexistent", serde_json::json!({"task": "x"}), &ctx)
        .await;
    assert!(err.is_err(), "delegating to nonexistent crew should fail");
}

/// 测试 ReActConfig 序列化/反序列化
#[test]
fn test_react_config_serde() {
    let config = ReActConfig {
        max_steps: 15,
        temperature: 0.3,
        enable_tools: true,
        allowed_tools: vec!["calculator".into(), "web_search".into()],
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: ReActConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.max_steps, 15);
    assert_eq!(deserialized.allowed_tools.len(), 2);
}

/// 测试 init_engine（预期在桩模式返回 Unsupported）
#[tokio::test]
async fn test_autoagents_init_engine() {
    let mut orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());

    match orch.init_engine("http://localhost:8080", "test-key").await {
        Ok(()) => {
            // feature 模式成功
        }
        Err(LsError::NotImplemented(_)) => {
            // 桩模式预期行为
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}
