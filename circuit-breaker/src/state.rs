//! 熔断器状态机：Closed ↔ Open ↔ HalfOpen.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

/// 熔断器状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// 关闭状态 — 正常流转.
    Closed,
    /// 断开状态 — 拒绝所有请求.
    Open,
    /// 半开状态 — 允许有限探测.
    HalfOpen,
}

impl fmt::Display for CircuitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "Closed"),
            Self::Open => write!(f, "Open"),
            Self::HalfOpen => write!(f, "HalfOpen"),
        }
    }
}

/// 状态变更记录.
#[derive(Debug, Clone)]
pub struct StateTransition {
    /// 变更前的状态.
    pub from: CircuitState,
    /// 变更后的状态.
    pub to: CircuitState,
    /// 变更时间戳（Unix 毫秒）.
    pub at_ms: i64,
    /// 变更原因.
    pub reason: String,
}

/// 状态机.
#[derive(Debug, Clone)]
pub struct StateMachine {
    current: CircuitState,
    last_state_change: Instant,
    half_open_start: Option<Instant>,
    half_open_probes_used: u64,
    max_half_open_probes: u64,
    half_open_timeout: Duration,
    last_transition: Option<StateTransition>,
}

impl StateMachine {
    /// 创建新的状态机.
    pub fn new(max_half_open_probes: u64, half_open_timeout_secs: u64) -> Self {
        Self {
            current: CircuitState::Closed,
            last_state_change: Instant::now(),
            half_open_start: None,
            half_open_probes_used: 0,
            max_half_open_probes,
            half_open_timeout: Duration::from_secs(half_open_timeout_secs),
            last_transition: None,
        }
    }

    /// 当前状态.
    pub fn current(&self) -> CircuitState {
        self.current
    }

    /// 上次状态变更的时间戳（Unix 毫秒）.
    pub fn last_state_change_at_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
            - self.last_state_change.elapsed().as_millis() as i64
    }

    /// 上次状态变更记录.
    pub fn last_transition(&self) -> Option<&StateTransition> {
        self.last_transition.as_ref()
    }

    /// 尝试转换到 Open 状态（Closed → Open）.
    pub fn try_open(&mut self, reason: String) -> CircuitState {
        let from = self.current;
        self.current = CircuitState::Open;
        self.last_state_change = Instant::now();
        self.half_open_start = None;
        self.half_open_probes_used = 0;
        self.last_transition = Some(StateTransition {
            from,
            to: CircuitState::Open,
            at_ms: chrono::Utc::now().timestamp_millis(),
            reason,
        });
        self.current
    }

    /// 检查是否应进入 HalfOpen（Open 超时后）.
    pub fn should_attempt_half_open(&self) -> bool {
        if self.current != CircuitState::Open {
            return false;
        }
        self.last_state_change.elapsed() >= self.half_open_timeout
    }

    /// 尝试进入 HalfOpen 状态.
    pub fn try_half_open(&mut self) -> CircuitState {
        if self.current != CircuitState::Open {
            return self.current;
        }
        let from = self.current;
        self.current = CircuitState::HalfOpen;
        self.last_state_change = Instant::now();
        self.half_open_start = Some(Instant::now());
        self.half_open_probes_used = 0;
        self.last_transition = Some(StateTransition {
            from,
            to: CircuitState::HalfOpen,
            at_ms: chrono::Utc::now().timestamp_millis(),
            reason: "Open timeout reached, attempting half-open".into(),
        });
        self.current
    }

    /// 是否允许发送探测请求（HalfOpen 状态下）.
    pub fn allow_probe(&mut self) -> bool {
        if self.current != CircuitState::HalfOpen {
            return false;
        }
        if self.half_open_probes_used < self.max_half_open_probes {
            self.half_open_probes_used += 1;
            true
        } else {
            false
        }
    }

    /// 探测成功 → 回到 Closed.
    pub fn record_probe_success(&mut self) -> CircuitState {
        if self.current != CircuitState::HalfOpen {
            return self.current;
        }
        let from = self.current;
        self.current = CircuitState::Closed;
        self.last_state_change = Instant::now();
        self.half_open_start = None;
        self.half_open_probes_used = 0;
        self.last_transition = Some(StateTransition {
            from,
            to: CircuitState::Closed,
            at_ms: chrono::Utc::now().timestamp_millis(),
            reason: "Probe succeeded, resetting to Closed".into(),
        });
        self.current
    }

    /// 探测失败 → 回到 Open.
    pub fn record_probe_failure(&mut self) -> CircuitState {
        if self.current != CircuitState::HalfOpen {
            return self.current;
        }
        let from = self.current;
        self.current = CircuitState::Open;
        self.last_state_change = Instant::now();
        self.half_open_start = None;
        self.half_open_probes_used = 0;
        self.last_transition = Some(StateTransition {
            from,
            to: CircuitState::Open,
            at_ms: chrono::Utc::now().timestamp_millis(),
            reason: "Probe failed, returning to Open".into(),
        });
        self.current
    }

    /// 手动重置到 Closed.
    pub fn reset(&mut self) {
        let from = self.current;
        self.current = CircuitState::Closed;
        self.last_state_change = Instant::now();
        self.half_open_start = None;
        self.half_open_probes_used = 0;
        self.last_transition = Some(StateTransition {
            from,
            to: CircuitState::Closed,
            at_ms: chrono::Utc::now().timestamp_millis(),
            reason: "Manual reset".into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = StateMachine::new(3, 30);
        assert_eq!(sm.current(), CircuitState::Closed);
    }

    #[test]
    fn test_open_transition() {
        let mut sm = StateMachine::new(3, 30);
        sm.try_open("too many failures".into());
        assert_eq!(sm.current(), CircuitState::Open);
    }

    #[test]
    fn test_half_open_probe_limit() {
        let mut sm = StateMachine::new(2, 30);
        // Simulate timeout by directly setting state
        let from = sm.current();
        sm.current = CircuitState::HalfOpen;
        sm.half_open_start = Some(Instant::now());
        sm.last_transition = Some(StateTransition {
            from,
            to: CircuitState::HalfOpen,
            at_ms: chrono::Utc::now().timestamp_millis(),
            reason: "test".into(),
        });

        assert!(sm.allow_probe());
        assert!(sm.allow_probe());
        assert!(!sm.allow_probe()); // exceeded max probes
    }

    #[test]
    fn test_probe_success_resets_to_closed() {
        let mut sm = StateMachine::new(3, 30);
        sm.current = CircuitState::HalfOpen; // force state
        let state = sm.record_probe_success();
        assert_eq!(state, CircuitState::Closed);
        assert_eq!(sm.current(), CircuitState::Closed);
    }

    #[test]
    fn test_probe_failure_returns_to_open() {
        let mut sm = StateMachine::new(3, 30);
        sm.current = CircuitState::HalfOpen;
        let state = sm.record_probe_failure();
        assert_eq!(state, CircuitState::Open);
        assert_eq!(sm.current(), CircuitState::Open);
    }

    #[test]
    fn test_reset() {
        let mut sm = StateMachine::new(3, 30);
        sm.try_open("test".into());
        sm.reset();
        assert_eq!(sm.current(), CircuitState::Closed);
    }
}
