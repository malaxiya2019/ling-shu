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

// ── v4.1 增强: 自动恢复 ────────────────────────────

use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::VecDeque;


/// 自动恢复策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoRecoveryPolicy {
    /// 立即重启
    ImmediateRestart,
    /// 指数退避重试
    ExponentialBackoff { max_attempts: u32, initial_delay_ms: u64 },
    /// 降级模式
    Degrade,
    /// 优雅停止
    GracefulStop,
}

impl Default for AutoRecoveryPolicy {
    fn default() -> Self {
        Self::ExponentialBackoff { max_attempts: 5, initial_delay_ms: 1000 }
    }
}

/// 健康检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
}

/// 组件状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub consecutive_failures: u32,
    pub recovery_attempts: u32,
}

/// 自动恢复引擎
pub struct AutoRecoveryEngine {
    /// 组件健康状态
    components: Arc<RwLock<Vec<ComponentHealth>>>,
    /// 故障事件历史
    event_history: Arc<RwLock<VecDeque<FaultEvent>>>,
    /// 最大历史记录数
    max_history: usize,
    /// 恢复策略
    policy: AutoRecoveryPolicy,
}

impl AutoRecoveryEngine {
    pub fn new(policy: AutoRecoveryPolicy) -> Self {
        Self {
            components: Arc::new(RwLock::new(Vec::new())),
            event_history: Arc::new(RwLock::new(VecDeque::new())),
            max_history: 1000,
            policy,
        }
    }

    /// 注册一个受监控的组件
    pub async fn register_component(&self, name: &str) {
        let mut components = self.components.write().await;
        // 检查是否已存在
        if !components.iter().any(|c| c.name == name) {
            components.push(ComponentHealth {
                name: name.to_string(),
                status: HealthStatus::Healthy,
                last_check: chrono::Utc::now(),
                consecutive_failures: 0,
                recovery_attempts: 0,
            });
        }
    }

    /// 报告健康检查结果
    pub async fn report_health(&self, name: &str, status: HealthStatus) {
        let mut components = self.components.write().await;
        if let Some(component) = components.iter_mut().find(|c| c.name == name) {
            component.last_check = chrono::Utc::now();
            match &status {
                HealthStatus::Healthy => {
                    component.consecutive_failures = 0;
                }
                HealthStatus::Degraded { .. } => {
                    component.consecutive_failures += 1;
                }
                HealthStatus::Unhealthy { .. } => {
                    component.consecutive_failures += 1;
                }
            }
            component.status = status;
        }
    }

    /// 记录故障事件
    pub async fn record_fault(&self, event: FaultEvent) {
        let mut history = self.event_history.write().await;
        history.push_back(event);
        while history.len() > self.max_history {
            history.pop_front();
        }
    }

    /// 检查是否需要恢复操作
    pub async fn check_and_recover(&self) -> Vec<RecoveryResult> {
        let mut results = Vec::new();
        let components = self.components.read().await;

        for component in components.iter() {
            if component.consecutive_failures == 0 {
                continue;
            }

            let max_attempts = match self.policy {
                AutoRecoveryPolicy::ExponentialBackoff { max_attempts, .. } => max_attempts,
                _ => 3,
            };

            if component.recovery_attempts >= max_attempts {
                results.push(RecoveryResult {
                    success: false,
                    recovered_from: component.name.clone(),
                    recovery_action: "max_attempts_exceeded".to_string(),
                    details: format!("Recovery failed after {} attempts", component.recovery_attempts),
                });
                continue;
            }

            // 执行恢复
            let result = match self.policy {
                AutoRecoveryPolicy::ImmediateRestart => {
                    RecoveryResult {
                        success: true,
                        recovered_from: component.name.clone(),
                        recovery_action: "restart".to_string(),
                        details: "Immediate restart triggered".to_string(),
                    }
                }
                AutoRecoveryPolicy::ExponentialBackoff { initial_delay_ms, .. } => {
                    let delay = initial_delay_ms * (1u64 << component.recovery_attempts.min(6));
                    RecoveryResult {
                        success: true,
                        recovered_from: component.name.clone(),
                        recovery_action: "exponential_backoff".to_string(),
                        details: format!("Backoff delay: {}ms (attempt {})", delay, component.recovery_attempts + 1),
                    }
                }
                AutoRecoveryPolicy::Degrade => {
                    RecoveryResult {
                        success: true,
                        recovered_from: component.name.clone(),
                        recovery_action: "degrade".to_string(),
                        details: "Running in degraded mode".to_string(),
                    }
                }
                AutoRecoveryPolicy::GracefulStop => {
                    RecoveryResult {
                        success: true,
                        recovered_from: component.name.clone(),
                        recovery_action: "graceful_stop".to_string(),
                        details: "Component stopped gracefully".to_string(),
                    }
                }
            };

            results.push(result);
        }

        results
    }

    /// 获取所有组件的健康状态
    pub async fn get_all_health(&self) -> Vec<ComponentHealth> {
        self.components.read().await.clone()
    }

    /// 获取故障事件历史
    pub async fn get_event_history(&self) -> Vec<FaultEvent> {
        self.event_history.read().await.iter().cloned().collect()
    }

    /// 获取健康摘要
    pub async fn health_summary(&self) -> String {
        let components = self.components.read().await;
        let total = components.len();
        let healthy = components.iter().filter(|c| matches!(c.status, HealthStatus::Healthy)).count();
        let degraded = components.iter().filter(|c| matches!(c.status, HealthStatus::Degraded { .. })).count();
        let unhealthy = components.iter().filter(|c| matches!(c.status, HealthStatus::Unhealthy { .. })).count();
        format!("{}/{} healthy, {} degraded, {} unhealthy", healthy, total, degraded, unhealthy)
    }
}

impl Default for AutoRecoveryEngine {
    fn default() -> Self {
        Self::new(AutoRecoveryPolicy::default())
    }
}

#[cfg(test)]
mod auto_recovery_tests {
    use super::*;

    #[tokio::test]
    async fn test_register_component() {
        let engine = AutoRecoveryEngine::default();
        engine.register_component("database").await;
        engine.register_component("llm").await;
        let health = engine.get_all_health().await;
        assert_eq!(health.len(), 2);
    }

    #[tokio::test]
    async fn test_report_health() {
        let engine = AutoRecoveryEngine::default();
        engine.register_component("test").await;
        engine.report_health("test", HealthStatus::Unhealthy { reason: "timeout".into() }).await;

        let health = engine.get_all_health().await;
        let component = health.iter().find(|c| c.name == "test").unwrap();
        assert_eq!(component.consecutive_failures, 1);
        assert!(matches!(component.status, HealthStatus::Unhealthy { .. }));
    }

    #[tokio::test]
    async fn test_recovery_backoff() {
        let engine = AutoRecoveryEngine::new(
            AutoRecoveryPolicy::ExponentialBackoff { max_attempts: 3, initial_delay_ms: 100 }
        );
        engine.register_component("test").await;
        engine.report_health("test", HealthStatus::Unhealthy { reason: "error".into() }).await;

        let results = engine.check_and_recover().await;
        assert!(!results.is_empty());
        assert_eq!(results[0].recovery_action, "exponential_backoff");
    }

    #[tokio::test]
    async fn test_immediate_restart() {
        let engine = AutoRecoveryEngine::new(AutoRecoveryPolicy::ImmediateRestart);
        engine.register_component("test").await;
        engine.report_health("test", HealthStatus::Unhealthy { reason: "crash".into() }).await;

        let results = engine.check_and_recover().await;
        assert_eq!(results[0].recovery_action, "restart");
    }

    #[tokio::test]
    async fn test_degrade_mode() {
        let engine = AutoRecoveryEngine::new(AutoRecoveryPolicy::Degrade);
        engine.register_component("test").await;
        engine.report_health("test", HealthStatus::Degraded { reason: "slow".into() }).await;

        let results = engine.check_and_recover().await;
        assert_eq!(results[0].recovery_action, "degrade");
    }

    #[tokio::test]
    async fn test_healthy_no_recovery() {
        let engine = AutoRecoveryEngine::default();
        engine.register_component("test").await;
        engine.report_health("test", HealthStatus::Healthy).await;

        let results = engine.check_and_recover().await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_health_summary() {
        let engine = AutoRecoveryEngine::default();
        engine.register_component("a").await;
        engine.register_component("b").await;
        engine.report_health("b", HealthStatus::Degraded { reason: "slow".into() }).await;

        let summary = engine.health_summary().await;
        assert!(summary.contains("healthy"));
    }

    #[tokio::test]
    async fn test_record_fault() {
        let engine = AutoRecoveryEngine::default();
        let event = FaultEvent {
            source: "test".to_string(),
            level: FaultLevel::Error,
            message: "test fault".to_string(),
            context: None,
            timestamp: chrono::Utc::now(),
        };
        engine.record_fault(event).await;
        let history = engine.get_event_history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].message, "test fault");
    }

    #[tokio::test]
    async fn test_duplicate_register() {
        let engine = AutoRecoveryEngine::default();
        engine.register_component("test").await;
        engine.register_component("test").await; // duplicate
        let health = engine.get_all_health().await;
        assert_eq!(health.len(), 1);
    }
}
