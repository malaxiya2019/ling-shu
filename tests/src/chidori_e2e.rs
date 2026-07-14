//! chidori 集成端到端测试
//!
//! 验证 ChidoriRecoveryManager 的断点保存、恢复和故障恢复流程。
//! 注意：此测试在 chidori feature 启用时运行真实逻辑，
//! 否则只验证桩模块不 panic。

use lingshu_core::{LsContext, LsError, LsId};
use lingshu_runtime::chidori_recovery::{
    CheckpointConfig, CheckpointRecovery, ChidoriRecoveryManager,
};
use lingshu_runtime::recovery::{FaultEvent, FaultLevel};

/// 测试 checkpoint 基本存/取流程
#[tokio::test]
async fn test_chidori_checkpoint_roundtrip() {
    let ctx = LsContext::with_session(LsId::new());
    let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());

    let state = b"agent-state-42".to_vec();
    let point_id = match manager
        .save_checkpoint(&ctx, "agent-e2e", state.clone())
        .await
    {
        Ok(id) => id,
        Err(e) if matches!(e, LsError::NotImplemented(_)) => {
            // 桩模式：验证返回正确的错误类型
            assert!(e.to_string().contains("chidori feature not enabled"));
            return;
        }
        Err(e) => panic!("unexpected error: {e}"),
    };

    // 验证 point_id 格式
    assert!(
        point_id.starts_with("cp-agent-e2e-"),
        "point_id should start with agent prefix, got: {point_id}"
    );

    // 恢复
    let restored = manager
        .restore_latest("agent-e2e")
        .await
        .expect("restore should succeed")
        .expect("restore should find checkpoint");
    assert_eq!(restored.state, state);
    assert_eq!(
        restored.metadata.get("agent_id").map(|s| s.as_str()),
        Some("agent-e2e")
    );

    // 列出断点
    let checkpoints = manager.list_checkpoints().await;
    assert_eq!(checkpoints.len(), 1);

    // 清除
    let removed = manager
        .clear_checkpoints("agent-e2e")
        .await
        .expect("clear should succeed");
    assert_eq!(removed, 1);

    // 验证已清除
    let remaining = manager.list_checkpoints().await;
    assert!(remaining.is_empty());
}

/// 测试故障恢复流程（checkpoint 优先）
#[tokio::test]
async fn test_chidori_fault_recovery() {
    let ctx = LsContext::with_session(LsId::new());
    let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());

    // 先保存 checkpoint
    let _ = manager
        .save_checkpoint(&ctx, "agent-recover", b"recovery-state".to_vec())
        .await;

    // 制造故障事件
    let event = FaultEvent {
        source: "agent-recover".into(),
        level: FaultLevel::Error,
        message: "simulated failure for recovery test".into(),
        context: None,
        timestamp: chrono::Utc::now(),
    };

    let result = manager
        .record_and_recover(&ctx, &event)
        .await
        .expect("recovery should not fail");

    match result {
        Some(r) => {
            // 成功恢复（真实 chidori 或桩）
            assert!(r.success);
        }
        None => {
            // 桩模式可能返回 None
        }
    }
}

/// 测试最大 checkpoint 数量限制
#[tokio::test]
async fn test_chidori_max_snapshots() {
    let config = CheckpointConfig {
        max_snapshots: 3,
        ..CheckpointConfig::default()
    };
    let ctx = LsContext::with_session(LsId::new());
    let manager = ChidoriRecoveryManager::new(config);

    for i in 0..5 {
        match manager
            .save_checkpoint(&ctx, "agent-limit", vec![i as u8])
            .await
        {
            Ok(_) => {}
            Err(LsError::NotImplemented(_)) => return,
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    let checkpoints = manager.list_checkpoints().await;
    assert!(
        checkpoints.len() <= 3,
        "max_snapshots=3 but got {} checkpoints",
        checkpoints.len()
    );
}

/// 测试 CheckpointRecovery 策略创建
#[test]
fn test_checkpoint_recovery_strategy() {
    let _strategy = CheckpointRecovery::new("agent-1");
    // 仅在 chidori feature 开启时有字段
    #[cfg(feature = "chidori")]
    {
        assert_eq!(strategy.agent_id, "agent-1");
        assert!(strategy.checkpoint_id.is_none());
    }

    #[cfg(feature = "chidori")]
    {
        let with_cp = CheckpointRecovery::new("agent-1").with_checkpoint("cp-abc");
        assert_eq!(with_cp.checkpoint_id, Some("cp-abc".into()));
    }
}

/// 测试 ChidoriRecoveryManager 的熔断器功能
#[tokio::test]
async fn test_chidori_circuit_breaker() {
    let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());

    // 初始状态：熔断器关闭
    assert!(!manager.is_circuit_open());

    // 桩模式下 reset 不应错误
    manager.reset_circuit_breaker().expect("reset should work");
    assert!(!manager.is_circuit_open());
}
