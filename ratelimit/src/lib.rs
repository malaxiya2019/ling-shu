//! LSRateLimit — Lingshu 速率限制.
//!
//! 提供令牌桶 (Token Bucket) 和滑动窗口 (Sliding Window) 两种限流算法，
//! 支持 per-key / per-user 粒度的速率控制。
//!
//! ## 架构
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │           RateLimiter (trait)            │
//! │  ┌──────────────────┐ ┌───────────────┐ │
//! │  │ TokenBucket      │ │ SlidingWindow │ │
//! │  │ (固定容量 + 填充) │ │ (时间窗口计数) │ │
//! │  └──────────────────┘ └───────────────┘ │
//! │  ┌──────────────────────────────────┐   │
//! │  │       RateLimitGuard             │   │
//! │  └──────────────────────────────────┘   │
//! └─────────────────────────────────────────┘
//! ```

pub mod bucket;
pub mod guard;
mod store;
pub mod window;

pub use bucket::TokenBucket;
pub use guard::{RateLimitDecision, RateLimitGuard, RateLimitRule};
pub use window::SlidingWindow;

use async_trait::async_trait;
use lingshu_core::LsResult;
use std::sync::Arc;

/// 限流结果.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitResult {
    /// 是否允许通过.
    pub allowed: bool,
    /// 剩余配额.
    pub remaining: u64,
    /// 重置时间戳（秒）.
    pub reset_at: u64,
    /// 总配额上限.
    pub limit: u64,
}

/// 限流器抽象 trait.
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// 检查 key 是否允许通过，消费一个配额.
    async fn check(&self, key: &str) -> LsResult<RateLimitResult>;

    /// 检查 key 是否允许通过，不消费配额（仅查看）.
    async fn peek(&self, key: &str) -> LsResult<RateLimitResult>;

    /// 重置某个 key 的限流状态.
    async fn reset(&self, key: &str) -> LsResult<()>;
}

/// 多策略组合限流器（可同时应用多个规则）.
#[derive(Clone)]
pub struct MultiRateLimiter {
    limiters: Vec<(String, Arc<dyn RateLimiter>)>,
}

impl MultiRateLimiter {
    pub fn new() -> Self {
        Self {
            limiters: Vec::new(),
        }
    }

    /// 添加一个命名限流器.
    pub fn add(&mut self, name: &str, limiter: Arc<dyn RateLimiter>) {
        self.limiters.push((name.to_string(), limiter));
    }

    /// 检查所有限流器，必须全部通过.
    pub async fn check_all(&self, key: &str) -> LsResult<RateLimitResult> {
        let mut worst = RateLimitResult {
            allowed: true,
            remaining: u64::MAX,
            reset_at: 0,
            limit: u64::MAX,
        };
        for (_name, limiter) in &self.limiters {
            let result = limiter.check(key).await?;
            if !result.allowed {
                return Ok(result);
            }
            if result.remaining < worst.remaining {
                worst = result;
            }
        }
        Ok(worst)
    }
}

impl Default for MultiRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}
