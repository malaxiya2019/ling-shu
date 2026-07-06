//! 滑动窗口 (Sliding Window) 限流器.
//!
//! 基于时间窗口 + 请求计数实现，支持固定窗口和滑动窗口两种模式。

use crate::{RateLimitResult, RateLimiter};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use lingshu_core::LsResult;

#[derive(Debug, Clone)]
struct WindowEntry {
    /// 当前窗口起始时间戳（毫秒）.
    window_start: i64,
    /// 当前窗口计数.
    count: u64,
    /// 上一个窗口计数（用于滑动窗口平滑）.
    prev_count: u64,
}

/// 滑动窗口限流器.
#[derive(Clone)]
pub struct SlidingWindow {
    /// 每个窗口的请求上限.
    limit: u64,
    /// 窗口大小（毫秒）.
    window_ms: i64,
    /// per-key 状态存储.
    state: DashMap<String, WindowEntry>,
}

impl SlidingWindow {
    /// 创建滑动窗口限流器.
    ///
    /// - `limit`: 每个时间窗口允许的最大请求数
    /// - `window_secs`: 窗口大小（秒）
    pub fn new(limit: u64, window_secs: u64) -> Self {
        Self {
            limit,
            window_ms: (window_secs * 1000) as i64,
            state: DashMap::new(),
        }
    }

    fn current_window_key() -> i64 {
        Utc::now().timestamp_millis()
    }

    fn calculate_usage(&self, entry: &WindowEntry) -> f64 {
        let now = Self::current_window_key();
        let elapsed = now - entry.window_start;

        if elapsed >= self.window_ms {
            return 0.0;
        }

        let weight = 1.0 - (elapsed as f64 / self.window_ms as f64);
        entry.prev_count as f64 * weight + entry.count as f64
    }
}

#[async_trait]
impl RateLimiter for SlidingWindow {
    async fn check(&self, key: &str) -> LsResult<RateLimitResult> {
        let now = Self::current_window_key();
        let window_start = (now / self.window_ms) * self.window_ms;

        let mut entry = self
            .state
            .entry(key.to_string())
            .or_insert_with(|| WindowEntry {
                window_start,
                count: 0,
                prev_count: 0,
            });

        let cur_start = (now / self.window_ms) * self.window_ms;
        if cur_start != entry.window_start {
            entry.prev_count = if cur_start - entry.window_start <= self.window_ms {
                entry.count
            } else {
                0
            };
            entry.window_start = cur_start;
            entry.count = 0;
        }

        let usage = self.calculate_usage(&entry);
        let allowed = usage < self.limit as f64;

        if allowed {
            entry.count += 1;
        }

        let remaining = (self.limit as f64 - self.calculate_usage(&entry)).max(0.0) as u64;
        let reset_at = ((window_start + self.window_ms) / 1000) as u64;

        Ok(RateLimitResult {
            allowed,
            remaining,
            reset_at,
            limit: self.limit,
        })
    }

    async fn peek(&self, key: &str) -> LsResult<RateLimitResult> {
        let now = Self::current_window_key();
        let window_start = (now / self.window_ms) * self.window_ms;

        let entry = self
            .state
            .entry(key.to_string())
            .or_insert_with(|| WindowEntry {
                window_start,
                count: 0,
                prev_count: 0,
            });

        let usage = self.calculate_usage(&entry);

        Ok(RateLimitResult {
            allowed: usage < self.limit as f64,
            remaining: (self.limit as f64 - usage).max(0.0) as u64,
            reset_at: ((window_start + self.window_ms) / 1000) as u64,
            limit: self.limit,
        })
    }

    async fn reset(&self, key: &str) -> LsResult<()> {
        self.state.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sliding_window_allows_within_limit() {
        let sw = SlidingWindow::new(5, 1);
        for _ in 0..5 {
            let r = sw.check("key").await.unwrap();
            assert!(r.allowed);
        }
    }

    #[tokio::test]
    async fn test_sliding_window_blocks_excess() {
        let sw = SlidingWindow::new(3, 1);
        for _ in 0..3 {
            sw.check("key").await.unwrap();
        }
        let r = sw.check("key").await.unwrap();
        assert!(!r.allowed);
        assert_eq!(r.remaining, 0);
    }

    #[tokio::test]
    async fn test_sliding_window_reset() {
        let sw = SlidingWindow::new(2, 10);
        sw.check("key").await.unwrap();
        sw.check("key").await.unwrap();
        let r = sw.check("key").await.unwrap();
        assert!(!r.allowed);

        sw.reset("key").await.unwrap();
        let r = sw.check("key").await.unwrap();
        assert!(r.allowed);
    }

    #[tokio::test]
    async fn test_sliding_window_isolated_keys() {
        let sw = SlidingWindow::new(2, 10);
        sw.check("alice").await.unwrap();
        sw.check("alice").await.unwrap();
        let r = sw.check("bob").await.unwrap();
        assert!(r.allowed);
    }

    #[tokio::test]
    async fn test_sliding_window_remaining() {
        let sw = SlidingWindow::new(10, 60);
        let r = sw.check("key").await.unwrap();
        assert!(r.allowed);
        assert_eq!(r.remaining, 9);
        assert_eq!(r.limit, 10);
    }
}
