//! LSCircuitBreaker — Lingshu 熔断器.
//!
//! 提供滑动窗口 + 半开检测的熔断器实现，保护下游服务免受级联故障影响。
//!
//! ## 状态机
//!
//! ```text
//!      ┌──────────┐
//!      │  Closed   │ ◄────────────┐
//!      └────┬─────┘              │
//!           │ 失败阈值触发         │
//!           ▼                    │
//!      ┌──────────┐   超时恢复   ┌──────────┐
//!      │   Open   │ ──────────► │ HalfOpen │
//!      └──────────┘             └────┬─────┘
//!                                    │
//!                                    │ 探测成功 → Closed
//!                                    │ 探测失败 → Open
//!                                    ▼
//!                               ┌──────────┐
//!                               │  Closed   │
//!                               └──────────┘
//! ```

pub mod breaker;
pub mod sliding_window;
pub mod state;

use async_trait::async_trait;
use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};

pub use breaker::DefaultCircuitBreaker;
pub use sliding_window::SlidingWindow;
pub use state::CircuitState;

// ──────────────────────────────────────
// 熔断器配置
// ──────────────────────────────────────

/// 熔断器配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// 滑动窗口大小（秒），默认 60.
    pub window_secs: u64,
    /// 窗口内允许的最大失败次数，默认 5.
    pub max_failures: u64,
    /// 进入 HalfOpen 后的超时时间（秒），默认 30.
    pub half_open_timeout_secs: u64,
    /// HalfOpen 状态下允许的最大探测请求数，默认 3.
    pub max_half_open_probes: u64,
    /// 最小请求数阈值（低于此数不触发熔断），默认 10.
    pub min_request_threshold: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            window_secs: 60,
            max_failures: 5,
            half_open_timeout_secs: 30,
            max_half_open_probes: 3,
            min_request_threshold: 10,
        }
    }
}

// ──────────────────────────────────────
// 结果类型
// ──────────────────────────────────────

/// 熔断器决策结果.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakerDecision {
    /// 允许请求通过.
    Allowed,
    /// 请求被熔断拒绝.
    Rejected,
    /// 请求作为探测请求通过（HalfOpen 状态下）.
    Probe,
}

/// 熔断器状态摘要（用于监控/暴露）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakerStatus {
    /// 当前状态.
    pub state: CircuitState,
    /// 滑动窗口内总请求数.
    pub total_requests: u64,
    /// 滑动窗口内失败数.
    pub failures: u64,
    /// 上次状态变更时间戳（Unix 毫秒）.
    pub last_state_change_at_ms: i64,
    /// 熔断器名称.
    pub name: String,
}

// ──────────────────────────────────────
// 熔断器 Trait
// ──────────────────────────────────────

/// 熔断器抽象 trait.
#[async_trait]
pub trait CircuitBreaker: Send + Sync {
    /// 获取熔断器名称.
    fn name(&self) -> &str;

    /// 当前状态.
    fn state(&self) -> CircuitState;

    /// 是否允许请求通过.
    async fn allow_request(&self) -> BreakerDecision;

    /// 记录成功.
    async fn record_success(&self);

    /// 记录失败.
    async fn record_failure(&self);

    /// 获取状态摘要.
    async fn status(&self) -> BreakerStatus;

    /// 手动重置熔断器到 Closed 状态.
    async fn reset(&self) -> LsResult<()>;
}
