//! 默认熔断器实现 — DefaultCircuitBreaker.

use async_trait::async_trait;
use lingshu_core::LsResult;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::sliding_window::SlidingWindow;
use crate::state::StateMachine;
use crate::{
    BreakerDecision, BreakerStatus, CircuitBreaker, CircuitBreakerConfig, CircuitState,
};

/// 默认熔断器实现.
pub struct DefaultCircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    state: RwLock<StateMachine>,
    failure_window: RwLock<SlidingWindow>,
    request_window: RwLock<SlidingWindow>,
}

impl DefaultCircuitBreaker {
    /// 创建新的熔断器.
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            state: RwLock::new(StateMachine::new(
                config.max_half_open_probes,
                config.half_open_timeout_secs,
            )),
            failure_window: RwLock::new(SlidingWindow::new(config.window_secs)),
            request_window: RwLock::new(SlidingWindow::new(config.window_secs)),
            config,
        }
    }

    /// 创建带默认配置的熔断器.
    pub fn new_default(name: impl Into<String>) -> Self {
        Self::new(name, CircuitBreakerConfig::default())
    }

    /// 检查是否满足熔断触发条件.
    async fn should_trip(&self) -> bool {
        let total = self.request_window.write().await.count();
        let failures = self.failure_window.write().await.count();

        // 低于最小请求阈值时不触发
        if total < self.config.min_request_threshold {
            return false;
        }

        failures >= self.config.max_failures
    }
}

#[async_trait]
impl CircuitBreaker for DefaultCircuitBreaker {
    fn name(&self) -> &str {
        &self.name
    }

    fn state(&self) -> CircuitState {
        CircuitState::Closed
    }

    async fn allow_request(&self) -> BreakerDecision {
        let mut state = self.state.write().await;

        match state.current() {
            CircuitState::Closed => {
                // 检查是否需要熔断
                if self.should_trip().await {
                    warn!(name = %self.name, "circuit breaker tripped: too many failures");
                    let _ = state.try_open("failure threshold reached".into());
                    BreakerDecision::Rejected
                } else {
                    debug!(name = %self.name, "request allowed (closed)");
                    self.request_window.write().await.record();
                    BreakerDecision::Allowed
                }
            }
            CircuitState::Open => {
                if state.should_attempt_half_open() {
                    debug!(name = %self.name, "attempting half-open");
                    state.try_half_open();
                    state.allow_probe(); // 消耗一个探测槽位
                    self.request_window.write().await.record();
                    BreakerDecision::Probe
                } else {
                    debug!(name = %self.name, "request rejected (open)");
                    BreakerDecision::Rejected
                }
            }
            CircuitState::HalfOpen => {
                if state.allow_probe() {
                    self.request_window.write().await.record();
                    BreakerDecision::Probe
                } else {
                    BreakerDecision::Rejected
                }
            }
        }
    }

    async fn record_success(&self) {
        let mut state = self.state.write().await;
        if state.current() == CircuitState::HalfOpen {
            debug!(name = %self.name, "probe succeeded, resetting to closed");
            state.record_probe_success();
            self.failure_window.write().await.clear();
            self.request_window.write().await.clear();
        }
    }

    async fn record_failure(&self) {
        self.failure_window.write().await.record();
        self.request_window.write().await.record();

        let mut state = self.state.write().await;
        if state.current() == CircuitState::HalfOpen {
            warn!(name = %self.name, "probe failed, returning to open");
            state.record_probe_failure();
            self.failure_window.write().await.clear();
            self.request_window.write().await.clear();
        }
    }

    async fn status(&self) -> BreakerStatus {
        let state = self.state.read().await;
        let failures = self.failure_window.write().await.count();
        let total = self.request_window.write().await.count();

        BreakerStatus {
            state: state.current(),
            total_requests: total,
            failures,
            last_state_change_at_ms: state.last_state_change_at_ms(),
            name: self.name.clone(),
        }
    }

    async fn reset(&self) -> LsResult<()> {
        let mut state = self.state.write().await;
        state.reset();
        self.failure_window.write().await.clear();
        self.request_window.write().await.clear();
        debug!(name = %self.name, "circuit breaker manually reset");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn config_for_test() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            window_secs: 10,
            max_failures: 3,
            half_open_timeout_secs: 1,
            max_half_open_probes: 2,
            min_request_threshold: 0,
        }
    }

    #[tokio::test]
    async fn test_initial_allows_requests() {
        let cb = DefaultCircuitBreaker::new("test", config_for_test());
        assert_eq!(cb.allow_request().await, BreakerDecision::Allowed);
    }

    #[tokio::test]
    async fn test_trips_after_failures() {
        let cb = DefaultCircuitBreaker::new("test", config_for_test());

        for _ in 0..3 {
            cb.record_failure().await;
        }

        // `allow_request` triggers the Open transition and returns Rejected
        assert_eq!(cb.allow_request().await, BreakerDecision::Rejected);
        // Subsequent calls also get Rejected (no timeout yet)
        assert_eq!(cb.allow_request().await, BreakerDecision::Rejected);
    }

    #[tokio::test]
    async fn test_half_open_transition() {
        let cb = DefaultCircuitBreaker::new("test", config_for_test());

        // Trigger circuit breaker
        for _ in 0..3 {
            cb.record_failure().await;
        }

        // First allow_request transitions from Closed → Open (returns Rejected)
        assert_eq!(cb.allow_request().await, BreakerDecision::Rejected);

        // Wait for half-open timeout
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // Now the Open → HalfOpen transition should happen, returning a Probe
        let decision = cb.allow_request().await;
        assert_eq!(decision, BreakerDecision::Probe);
    }

    #[tokio::test]
    async fn test_probe_success_resets() {
        let cb = DefaultCircuitBreaker::new("test", config_for_test());

        // Trigger circuit breaker
        for _ in 0..3 {
            cb.record_failure().await;
        }
        assert_eq!(cb.allow_request().await, BreakerDecision::Rejected);

        // Wait for half-open
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert_eq!(cb.allow_request().await, BreakerDecision::Probe);

        // Record success → reset to Closed
        cb.record_success().await;

        // Should be closed again
        assert_eq!(cb.allow_request().await, BreakerDecision::Allowed);
    }

    #[tokio::test]
    async fn test_probe_failure_returns_to_open() {
        let cb = DefaultCircuitBreaker::new("test", config_for_test());

        // Trigger and enter half-open
        for _ in 0..3 {
            cb.record_failure().await;
        }
        assert_eq!(cb.allow_request().await, BreakerDecision::Rejected);
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let _ = cb.allow_request().await; // becomes a probe

        // Probe fails → back to Open
        cb.record_failure().await;

        // Should be open again
        assert_eq!(cb.allow_request().await, BreakerDecision::Rejected);
    }

    #[tokio::test]
    async fn test_reset() {
        let cb = DefaultCircuitBreaker::new("test", config_for_test());

        for _ in 0..3 {
            cb.record_failure().await;
        }
        assert_eq!(cb.allow_request().await, BreakerDecision::Rejected);

        cb.reset().await.unwrap();
        assert_eq!(cb.allow_request().await, BreakerDecision::Allowed);
    }

    #[tokio::test]
    async fn test_status() {
        let cb = DefaultCircuitBreaker::new("test", config_for_test());
        let status = cb.status().await;
        assert_eq!(status.state, CircuitState::Closed);
        assert_eq!(status.name, "test");
    }

    #[tokio::test]
    async fn test_probe_limit() {
        let cb = DefaultCircuitBreaker::new("test", config_for_test());

        for _ in 0..3 {
            cb.record_failure().await;
        }
        assert_eq!(cb.allow_request().await, BreakerDecision::Rejected);
        tokio::time::sleep(Duration::from_millis(1100)).await;

        // First probe
        assert_eq!(cb.allow_request().await, BreakerDecision::Probe);
        // Second probe (max_half_open_probes = 2)
        assert_eq!(cb.allow_request().await, BreakerDecision::Probe);
        // Should be rejected after exhausting probes
        assert_eq!(cb.allow_request().await, BreakerDecision::Rejected);
    }
}
