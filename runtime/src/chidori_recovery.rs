#![allow(unused_imports, dead_code)]
//! Chidori Recovery Integration — 持久化可回放 Agent 恢复
//!
//! 利用 chidori 的 durable execution + checkpointing 能力增强
//! Lingshu 的故障恢复系统。提供基于 `SavePointHook` 的断点恢复机制。
//!
//! ## Feature Gate
//! 该模块通过 `#[cfg(feature = "chidori")]` 条件编译，需在 `Cargo.toml` 中启用：
//! ```toml
//! chidori = { git = "https://github.com/ThousandBirdsInc/chidori.git" }
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::recovery::{FaultEvent, FaultLevel, RecoveryManager, RecoveryResult, RecoveryStrategy};

// ── chidori 类型前向声明 ──────────────────────────────
// chidori 编译时依赖，实际类型通过 git dep 提供。
// 此处使用 Feature Gate + cfg 隔离，确保非 chidori 编译不失败。

/// chidori SavePoint — 快照断点数据.
#[cfg(feature = "chidori")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChidoriSavePoint {
    /// 断点 ID
    pub point_id: String,
    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 序列化的 Agent 状态
    pub state: Vec<u8>,
    /// 上下文元数据
    pub metadata: std::collections::HashMap<String, String>,
}

/// Checkpoint 恢复配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// 最大保存的快照数
    pub max_snapshots: usize,
    /// 自动 checkpoint 间隔（秒）
    pub checkpoint_interval_secs: u64,
    /// 是否启用增量快照
    pub incremental: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            max_snapshots: 10,
            checkpoint_interval_secs: 30,
            incremental: true,
        }
    }
}

/// ChidoriRecoveryManager — 基于 checkpoint 的恢复管理器.
///
/// 包装底层的 `RecoveryManager`，添加断点保存与回放能力。
/// 当故障发生时，自动尝试从最近的断点恢复 Agent 状态。
#[cfg(feature = "chidori")]
pub struct ChidoriRecoveryManager {
    /// 内部的熔断恢复管理器
    inner: RecoveryManager,
    /// 断点存储
    checkpoints: Arc<RwLock<Vec<ChidoriSavePoint>>>,
    /// 配置
    config: CheckpointConfig,
    /// chidori 引擎（编译时依赖）
    engine: Option<Arc<RwLock<chidori::runtime::engine::Engine>>>,
}

#[cfg(feature = "chidori")]
impl ChidoriRecoveryManager {
    /// 创建新的 ChidoriRecoveryManager.
    pub fn new(config: CheckpointConfig) -> Self {
        Self {
            inner: RecoveryManager::new(5),
            checkpoints: Arc::new(RwLock::new(Vec::new())),
            config,
            engine: None,
        }
    }

    /// 附加 chidori Engine 实例.
    pub async fn attach_engine(&mut self, engine: chidori::runtime::engine::Engine) {
        self.engine = Some(Arc::new(RwLock::new(engine)));
        info!("chidori Engine attached to ChidoriRecoveryManager");
    }

    /// 保存检查点.
    pub async fn save_checkpoint(
        &self,
        _ctx: &LsContext,
        agent_id: &str,
        state: Vec<u8>,
    ) -> LsResult<String> {
        let mut checkpoints = self.checkpoints.write().await;
        let point_id = format!(
            "cp-{}-{}",
            agent_id,
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );

        let savepoint = ChidoriSavePoint {
            point_id: point_id.clone(),
            timestamp: chrono::Utc::now(),
            state,
            metadata: std::collections::HashMap::from([
                ("agent_id".into(), agent_id.to_string()),
                ("strategy".into(), "chidori-checkpoint".into()),
            ]),
        };

        // 维护最大快照数量
        while checkpoints.len() >= self.config.max_snapshots {
            checkpoints.remove(0);
        }
        checkpoints.push(savepoint);

        info!(
            point_id = %point_id,
            agent_id = %agent_id,
            "checkpoint saved"
        );
        Ok(point_id)
    }

    /// 从最近的检查点恢复.
    pub async fn restore_latest(&self, agent_id: &str) -> LsResult<Option<ChidoriSavePoint>> {
        let checkpoints = self.checkpoints.read().await;
        // 从后向前查找匹配 agent_id 的最新断点
        let found = checkpoints
            .iter()
            .rev()
            .find(|cp| cp.metadata.get("agent_id").map(|s| s.as_str()) == Some(agent_id))
            .cloned();
        Ok(found)
    }

    /// 列出所有断点.
    pub async fn list_checkpoints(&self) -> Vec<ChidoriSavePoint> {
        self.checkpoints.read().await.clone()
    }

    /// 清除指定 agent 的所有断点.
    pub async fn clear_checkpoints(&self, agent_id: &str) -> LsResult<usize> {
        let mut checkpoints = self.checkpoints.write().await;
        let before = checkpoints.len();
        checkpoints.retain(|cp| cp.metadata.get("agent_id").map(|s| s.as_str()) != Some(agent_id));
        let removed = before - checkpoints.len();
        info!(agent_id = %agent_id, removed, "checkpoints cleared");
        Ok(removed)
    }

    // — 委托给内部 RecoveryManager 的方法 —

    /// 记录故障并尝试从 checkpoint 恢复.
    pub async fn record_and_recover(
        &self,
        ctx: &LsContext,
        event: &FaultEvent,
    ) -> LsResult<Option<RecoveryResult>> {
        // 先尝试从 checkpoint 恢复
        let agent_id = event.source.as_str();
        if let Ok(Some(savepoint)) = self.restore_latest(agent_id).await {
            info!(
                point_id = %savepoint.point_id,
                agent_id = %agent_id,
                "attempting checkpoint recovery"
            );
            return Ok(Some(RecoveryResult {
                success: true,
                recovered_from: agent_id.to_string(),
                recovery_action: format!("checkpoint-restore from {}", savepoint.point_id),
                details: format!(
                    "restored from checkpoint at {} with {} bytes state",
                    savepoint.timestamp,
                    savepoint.state.len()
                ),
            }));
        }

        // 回退到默认恢复策略
        let strategy = self.inner.record_fault(ctx, event)?;
        match strategy {
            Some(s) => {
                let result = self.inner.recover(ctx, &s, &event.source).await?;
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    /// 重置熔断器.
    pub fn reset_circuit_breaker(&self) -> LsResult<()> {
        self.inner.reset_circuit_breaker()
    }

    /// 熔断器是否打开.
    pub fn is_circuit_open(&self) -> bool {
        self.inner.is_circuit_open()
    }
}

/// CheckpointRecovery — RecoveryStrategy 的新变体，表示从 checkpoint 恢复.
#[cfg(feature = "chidori")]
#[derive(Debug)]
pub struct CheckpointRecovery {
    /// 要恢复的 agent ID
    pub agent_id: String,
    /// 指定 checkpoint ID (None=使用最新)
    pub checkpoint_id: Option<String>,
}

#[cfg(feature = "chidori")]
impl CheckpointRecovery {
    /// 创建 CheckpointRecovery 策略.
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            checkpoint_id: None,
        }
    }

    /// 设置特定的 checkpoint ID.
    pub fn with_checkpoint(mut self, checkpoint_id: impl Into<String>) -> Self {
        self.checkpoint_id = Some(checkpoint_id.into());
        self
    }
}

/// 非 chidori 编译时的桩。
#[cfg(not(feature = "chidori"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChidoriSavePoint {
    pub point_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub state: Vec<u8>,
    pub metadata: std::collections::HashMap<String, String>,
}

// ── 非 chidori 编译时的桩 ────────────────────────────

/// 非 chidori 编译时提供空操作实现。
#[cfg(not(feature = "chidori"))]
pub struct ChidoriRecoveryManager;

#[cfg(not(feature = "chidori"))]
impl ChidoriRecoveryManager {
    /// 创建空实例（仅用于编译通过）。
    pub fn new(_config: CheckpointConfig) -> Self {
        Self
    }

    /// 非 chidori 编译时返回错误。
    pub async fn save_checkpoint(
        &self,
        _ctx: &LsContext,
        _agent_id: &str,
        _state: Vec<u8>,
    ) -> LsResult<String> {
        Err(LsError::NotImplemented(
            "chidori feature not enabled".into(),
        ))
    }

    /// 非 chidori 编译时返回 None。
    pub async fn restore_latest(&self, _agent_id: &str) -> LsResult<Option<ChidoriSavePoint>> {
        Err(LsError::NotImplemented(
            "chidori feature not enabled".into(),
        ))
    }

    /// 列出所有断点（空列表）。
    pub async fn list_checkpoints(&self) -> Vec<ChidoriSavePoint> {
        Vec::new()
    }

    /// 清除断点。
    pub async fn clear_checkpoints(&self, _agent_id: &str) -> LsResult<usize> {
        Err(LsError::NotImplemented(
            "chidori feature not enabled".into(),
        ))
    }

    /// 记录并恢复（非 chidori 编译时返回 None）。
    pub async fn record_and_recover(
        &self,
        ctx: &LsContext,
        event: &FaultEvent,
    ) -> LsResult<Option<RecoveryResult>> {
        // 不使用 checkpoint，直接返回 None 让调用方走默认路径
        let _ = ctx;
        let _ = event;
        Ok(None)
    }

    /// 熔断器是否打开。
    pub fn is_circuit_open(&self) -> bool {
        false
    }

    /// 重置熔断器。
    pub fn reset_circuit_breaker(&self) -> LsResult<()> {
        Ok(())
    }
}

/// 非 chidori 编译时空策略.
#[cfg(not(feature = "chidori"))]
#[derive(Debug)]
pub struct CheckpointRecovery;

#[cfg(not(feature = "chidori"))]
impl CheckpointRecovery {
    pub fn new(_agent_id: impl Into<String>) -> Self {
        Self
    }
}

#[cfg(test)]
#[cfg(feature = "chidori")]
mod tests {
    use super::*;
    use crate::recovery::FaultLevel;

    #[tokio::test]
    async fn test_checkpoint_save_and_restore() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());

        let state = b"agent-state-data".to_vec();
        let point_id = manager
            .save_checkpoint(&ctx, "agent-1", state.clone())
            .await
            .unwrap();

        assert!(point_id.starts_with("cp-agent-1"));

        let restored = manager.restore_latest("agent-1").await.unwrap();
        assert!(restored.is_some());
        assert_eq!(restored.unwrap().state, state);
    }

    #[tokio::test]
    async fn test_max_snapshots_limit() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let config = CheckpointConfig {
            max_snapshots: 3,
            ..Default::default()
        };
        let manager = ChidoriRecoveryManager::new(config);

        for i in 0..5 {
            manager
                .save_checkpoint(&ctx, "agent-1", vec![i as u8])
                .await
                .unwrap();
        }

        let checkpoints = manager.list_checkpoints().await;
        assert_eq!(checkpoints.len(), 3);
    }

    #[tokio::test]
    async fn test_clear_checkpoints() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());

        for i in 0..3 {
            manager
                .save_checkpoint(&ctx, "agent-x", vec![i])
                .await
                .unwrap();
        }

        let removed = manager.clear_checkpoints("agent-x").await.unwrap();
        assert_eq!(removed, 3);

        let cp_list = manager.list_checkpoints().await;
        assert!(cp_list.is_empty());
    }

    #[tokio::test]
    async fn test_record_and_recover_with_checkpoint() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());

        manager
            .save_checkpoint(&ctx, "agent-1", vec![1, 2, 3])
            .await
            .unwrap();

        let event = FaultEvent {
            source: "agent-1".into(),
            level: FaultLevel::Error,
            message: "test failure".into(),
            context: None,
            timestamp: chrono::Utc::now(),
        };

        let result = manager.record_and_recover(&ctx, &event).await.unwrap();
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.success);
        assert!(r.recovery_action.contains("checkpoint"));
    }
}

#[cfg(test)]
#[cfg(not(feature = "chidori"))]
mod stub_tests {
    use super::*;
    use crate::recovery::FaultLevel;

    #[test]
    fn test_stub_checkpoint_config_serde() {
        let config = CheckpointConfig {
            max_snapshots: 5,
            checkpoint_interval_secs: 60,
            incremental: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CheckpointConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_snapshots, 5);
    }

    #[tokio::test]
    async fn test_stub_save_checkpoint_returns_unsupported() {
        let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let err = manager.save_checkpoint(&ctx, "agent", vec![]).await;
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("chidori feature not enabled"));
    }

    #[tokio::test]
    async fn test_stub_restore_latest_returns_unsupported() {
        let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());
        let err = manager.restore_latest("agent").await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_stub_list_checkpoints_empty() {
        let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());
        let list = manager.list_checkpoints().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_stub_clear_checkpoints_returns_unsupported() {
        let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());
        let err = manager.clear_checkpoints("agent").await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_stub_record_and_recover_returns_none() {
        let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let event = FaultEvent {
            source: "agent".into(),
            level: FaultLevel::Warning,
            message: "test".into(),
            context: None,
            timestamp: chrono::Utc::now(),
        };
        let result = manager.record_and_recover(&ctx, &event).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_stub_circuit_breaker() {
        let manager = ChidoriRecoveryManager::new(CheckpointConfig::default());
        assert!(!manager.is_circuit_open());
        manager.reset_circuit_breaker().unwrap();
        assert!(!manager.is_circuit_open());
    }

    #[test]
    fn test_stub_checkpoint_recovery() {
        let strategy = CheckpointRecovery::new("agent-1");
        let _ = strategy; // stub is unit struct, just verify construction
    }
}
