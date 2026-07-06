//! 限流状态存储后端抽象.
//!
//! 当前实现使用内存存储，后续可扩展 Redis 后端。

use async_trait::async_trait;
use lingshu_core::LsResult;

/// 限流计数器.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RateCounter {
    pub count: u64,
    pub window_start: i64,
}

/// 限流状态存储 trait.
#[async_trait]
#[allow(dead_code)]
pub trait RateLimitStore: Send + Sync {
    /// 获取并递增计数器.
    async fn increment(&self, key: &str, window_ms: i64) -> LsResult<RateCounter>;

    /// 获取当前计数器（不修改）.
    async fn get(&self, key: &str) -> LsResult<Option<RateCounter>>;

    /// 删除计数器.
    async fn delete(&self, key: &str) -> LsResult<()>;
}
