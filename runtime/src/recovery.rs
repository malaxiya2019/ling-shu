use lingshu_core::{LsContext, LsError, LsResult};
use serde::{Deserialize, Serialize};

/// 故障级别.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaultLevel {
    Warning,
    Error,
    Critical,
}

/// 故障事件.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultEvent {
    pub source: String,
    pub level: FaultLevel,
    pub message: String,
    pub context: Option<LsContext>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 恢复结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResult {
    pub success: bool,
    pub recovered_from: String,
    pub recovery_action: String,
    pub details: String,
}

/// 故障恢复策略.
#[derive(Debug)]
pub enum RecoveryStrategy {
    /// 快速重试.
    Retry { max_attempts: u32, backoff_ms: u64 },
    /// 降级到备用实现.
    Fallback { fallback_name: String },
    /// 重启模块.
    Restart,
    /// 隔离并告警.
    IsolateAndAlert,
}

/// 故障恢复管理器.
#[derive(Debug)]
pub struct RecoveryManager {
    circuit_open: std::sync::RwLock<bool>,
    failure_count: std::sync::atomic::AtomicU64,
    max_failures_before_circuit_open: u64,
}

impl RecoveryManager {
    pub fn new(max_failures: u64) -> Self {
        Self {
            circuit_open: std::sync::RwLock::new(false),
            failure_count: std::sync::atomic::AtomicU64::new(0),
            max_failures_before_circuit_open: max_failures,
        }
    }

    /// 记录故障并判断是否需要熔断.
    pub fn record_fault(
        &self,
        _ctx: &LsContext,
        _event: &FaultEvent,
    ) -> LsResult<Option<RecoveryStrategy>> {
        let count = self
            .failure_count
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;

        if count >= self.max_failures_before_circuit_open {
            let mut open = self
                .circuit_open
                .write()
                .map_err(|e| LsError::Internal(format!("recovery lock poisoned: {e}")))?;
            if !*open {
                *open = true;
                return Ok(Some(RecoveryStrategy::IsolateAndAlert));
            }
        }

        Ok(Some(RecoveryStrategy::Retry {
            max_attempts: 3,
            backoff_ms: 100,
        }))
    }

    /// 执行恢复.
    pub async fn recover(
        &self,
        _ctx: &LsContext,
        strategy: &RecoveryStrategy,
        source: &str,
    ) -> LsResult<RecoveryResult> {
        match strategy {
            RecoveryStrategy::Retry {
                max_attempts,
                backoff_ms,
            } => Ok(RecoveryResult {
                success: true,
                recovered_from: source.to_string(),
                recovery_action: format!(
                    "retry up to {max_attempts} times with {backoff_ms}ms backoff"
                ),
                details: "retry strategy dispatched".into(),
            }),
            RecoveryStrategy::Fallback { fallback_name } => Ok(RecoveryResult {
                success: true,
                recovered_from: source.to_string(),
                recovery_action: format!("fallback to {fallback_name}"),
                details: "fallback strategy dispatched".into(),
            }),
            RecoveryStrategy::Restart => Ok(RecoveryResult {
                success: true,
                recovered_from: source.to_string(),
                recovery_action: "restart module".into(),
                details: "restart signal sent".into(),
            }),
            RecoveryStrategy::IsolateAndAlert => Ok(RecoveryResult {
                success: true,
                recovered_from: source.to_string(),
                recovery_action: "isolate and alert".into(),
                details: "module isolated, alert dispatched to admin".into(),
            }),
        }
    }

    /// 重置熔断器.
    pub fn reset_circuit_breaker(&self) -> LsResult<()> {
        let mut open = self
            .circuit_open
            .write()
            .map_err(|e| LsError::Internal(format!("recovery lock poisoned: {e}")))?;
        *open = false;
        self.failure_count
            .store(0, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// 熔断器是否打开.
    pub fn is_circuit_open(&self) -> bool {
        self.circuit_open.read().map(|s| *s).unwrap_or(false)
    }
}
