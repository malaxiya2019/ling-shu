//! 令牌桶 (Token Bucket) 限流器.
//!
//! 以恒定速率向桶中填充令牌，每个请求消耗一个令牌。
//! 支持突发流量（桶容量即最大突发值）。

use crate::{RateLimitResult, RateLimiter};
use async_trait::async_trait;
use chrono::Utc;
use lingshu_core::{LsError, LsResult};
use std::sync::Arc;
use tokio::sync::RwLock;

struct TokenBucketInner {
    /// 当前令牌数.
    tokens: f64,
    /// 桶容量（最大令牌数）.
    capacity: f64,
    /// 填充速率（令牌/秒）.
    fill_rate: f64,
    /// 上次填充时间戳（秒）.
    last_refill: f64,
}

/// 令牌桶限流器.
#[derive(Clone)]
pub struct TokenBucket {
    inner: Arc<RwLock<TokenBucketInner>>,
}

impl TokenBucket {
    /// 创建新令牌桶.
    ///
    /// - `capacity`: 桶容量（允许的最大突发请求数）
    /// - `fill_rate`: 每秒恢复的令牌数
    pub fn new(capacity: u64, fill_rate: f64) -> Self {
        let now = Utc::now().timestamp() as f64;
        Self {
            inner: Arc::new(RwLock::new(TokenBucketInner {
                tokens: capacity as f64,
                capacity: capacity as f64,
                fill_rate,
                last_refill: now,
            })),
        }
    }

    async fn refill(&self) -> f64 {
        let mut inner = self.inner.write().await;
        let now = Utc::now().timestamp() as f64;
        let elapsed = (now - inner.last_refill).max(0.0);
        let new_tokens = elapsed * inner.fill_rate;
        inner.tokens = (inner.tokens + new_tokens).min(inner.capacity);
        inner.last_refill = now;
        inner.tokens
    }
}

#[async_trait]
impl RateLimiter for TokenBucket {
    async fn check(&self, _key: &str) -> LsResult<RateLimitResult> {
        self.refill().await;
        let mut inner = self.inner.write().await;

        let allowed = inner.tokens >= 1.0;
        if allowed {
            inner.tokens -= 1.0;
        }

        let reset_at = (inner.last_refill + (inner.capacity - inner.tokens) / inner.fill_rate) as u64;

        Ok(RateLimitResult {
            allowed,
            remaining: inner.tokens.floor() as u64,
            reset_at,
            limit: inner.capacity as u64,
        })
    }

    async fn peek(&self, _key: &str) -> LsResult<RateLimitResult> {
        let tokens = self.refill().await;

        Ok(RateLimitResult {
            allowed: tokens >= 1.0,
            remaining: tokens.floor() as u64,
            reset_at: (Utc::now().timestamp() as f64 + (1.0 - tokens) / self.inner.read().await.fill_rate) as u64,
            limit: self.inner.read().await.capacity as u64,
        })
    }

    async fn reset(&self, _key: &str) -> LsResult<()> {
        let mut inner = self.inner.write().await;
        inner.tokens = inner.capacity;
        inner.last_refill = Utc::now().timestamp() as f64;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_bucket_allows_initial_burst() {
        let bucket = TokenBucket::new(10, 1.0);
        let result = bucket.check("key").await.unwrap();
        assert!(result.allowed);
        assert_eq!(result.remaining, 9);
        assert_eq!(result.limit, 10);
    }

    #[tokio::test]
    async fn test_token_bucket_exhausts() {
        let bucket = TokenBucket::new(3, 1.0);
        for _ in 0..3 {
            let r = bucket.check("key").await.unwrap();
            assert!(r.allowed);
        }
        let r = bucket.check("key").await.unwrap();
        assert!(!r.allowed);
        assert_eq!(r.remaining, 0);
    }

    #[tokio::test]
    async fn test_token_bucket_reset() {
        let bucket = TokenBucket::new(5, 10.0);
        for _ in 0..5 {
            bucket.check("key").await.unwrap();
        }
        let r = bucket.check("key").await.unwrap();
        assert!(!r.allowed);

        bucket.reset("key").await.unwrap();
        let r = bucket.check("key").await.unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 4);
    }

    #[tokio::test]
    async fn test_token_bucket_peek() {
        let bucket = TokenBucket::new(5, 1.0);
        let r = bucket.peek("key").await.unwrap();
        assert!(r.allowed);
        // peek should not consume
        let r = bucket.check("key").await.unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 4);
    }
}
