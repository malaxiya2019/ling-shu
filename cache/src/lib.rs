//! LSCache — LLM 响应缓存层。
//!
//! 提供统一的缓存抽象，支持 Redis / Memcached / In-Memory 后端，
//! 通过 feature flag 条件编译。
//!
//! ## Feature Flags
//! - `redis-cache` — Redis 后端 (依赖 `redis` crate)
//! - `memcached-cache` — Memcached 后端 (依赖 `memcache` crate)
//! - `full` — 启用所有后端
//!
//! ## 使用示例
//! ```rust,ignore
//! use lingshu_cache::CacheLayer;
//!
//! let cache = CacheLayer::in_memory();
//! cache.set("key", "value", 300).await?;
//! let val: Option<String> = cache.get("key").await?;
//! ```

mod memory;

use async_trait::async_trait;
use lingshu_core::{LsError, LsResult};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

pub use memory::InMemoryCache;

// ── Core Cache Trait ──────────────────────────────────────────────

/// 缓存后端统一接口.
#[async_trait]
pub trait CacheBackend: Send + Sync + Debug {
    /// 获取缓存值.
    async fn get_raw(&self, key: &str) -> LsResult<Option<Vec<u8>>>;

    /// 设置缓存值 (TTL 秒).
    async fn set_raw(&self, key: &str, value: &[u8], ttl_secs: u64) -> LsResult<()>;

    /// 删除缓存键.
    async fn delete(&self, key: &str) -> LsResult<bool>;

    /// 检查键是否存在.
    async fn exists(&self, key: &str) -> LsResult<bool>;

    /// 清空所有缓存 (谨慎使用).
    async fn clear(&self) -> LsResult<()>;

    /// 返回缓存统计信息.
    fn stats(&self) -> CacheStats;
}

/// 缓存统计.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub keys: usize,
    pub backend: &'static str,
}

// ── CacheLayer — 高层封装 ───────────────────────────────────────

/// 缓存层 — 提供类型安全的 get/set 操作.
#[derive(Debug, Clone)]
pub struct CacheLayer {
    backend: Arc<dyn CacheBackend>,
    key_prefix: String,
}

impl CacheLayer {
    /// 使用 In-Memory 后端创建缓存层.
    pub fn in_memory() -> Self {
        Self {
            backend: Arc::new(InMemoryCache::new()),
            key_prefix: String::new(),
        }
    }

    /// 使用指定后端创建缓存层.
    pub fn new(backend: Arc<dyn CacheBackend>) -> Self {
        Self {
            backend,
            key_prefix: String::new(),
        }
    }

    /// 设置键前缀 (用于多租户隔离).
    pub fn with_prefix(mut self, prefix: &str) -> Self {
        self.key_prefix = prefix.to_string();
        self
    }

    /// 构建完整的缓存键.
    fn build_key(&self, key: &str) -> String {
        if self.key_prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}:{}", self.key_prefix, key)
        }
    }

    /// 获取并反序列化缓存值.
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> LsResult<Option<T>> {
        let full_key = self.build_key(key);
        match self.backend.get_raw(&full_key).await {
            Ok(Some(data)) => {
                let value: T = serde_json::from_slice(&data)
                    .map_err(|e| LsError::Internal(format!("cache deserialize: {e}")))?;
                Ok(Some(value))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 序列化并设置缓存值.
    pub async fn set<T: Serialize>(&self, key: &str, value: &T, ttl_secs: u64) -> LsResult<()> {
        let full_key = self.build_key(key);
        let data = serde_json::to_vec(value)
            .map_err(|e| LsError::Internal(format!("cache serialize: {e}")))?;
        self.backend.set_raw(&full_key, &data, ttl_secs).await
    }

    /// 删除缓存键.
    pub async fn delete(&self, key: &str) -> LsResult<bool> {
        let full_key = self.build_key(key);
        self.backend.delete(&full_key).await
    }

    /// 检查键是否存在.
    pub async fn exists(&self, key: &str) -> LsResult<bool> {
        let full_key = self.build_key(key);
        self.backend.exists(&full_key).await
    }

    /// 清空所有缓存.
    pub async fn clear(&self) -> LsResult<()> {
        self.backend.clear().await
    }

    /// 获取缓存统计.
    pub fn stats(&self) -> CacheStats {
        self.backend.stats()
    }
}

// ── LLM 响应缓存特化 ─────────────────────────────────────────────

/// LLM 响应缓存键生成.
pub fn llm_cache_key(model: &str, messages_hash: &str, temperature: u64) -> String {
    format!("llm:{}:{}:t{}", model, messages_hash, temperature)
}

/// 计算消息列表的哈希值，用于缓存键.
pub fn hash_messages(messages: &[impl AsRef<str>]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for msg in messages {
        hasher.update(msg.as_ref().as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// LLM 缓存包装器 — 在 LLM 调用前检查缓存，未命中时调用后端并缓存结果.
pub struct LlmCache {
    pub cache: CacheLayer,
    pub default_ttl: u64,
    pub enabled: bool,
}

impl LlmCache {
    pub fn new(cache: CacheLayer) -> Self {
        Self {
            cache,
            default_ttl: 3600, // 1 hour default
            enabled: true,
        }
    }

    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.default_ttl = ttl_secs;
        self
    }

    pub fn disabled() -> Self {
        Self {
            cache: CacheLayer::in_memory(),
            default_ttl: 0,
            enabled: false,
        }
    }
}

// ── Redis Backend ─────────────────────────────────────────────────

#[cfg(feature = "redis-cache")]
pub mod redis_backend {
    use super::*;
    use redis::AsyncCommands;

    /// Redis 缓存后端.
    #[derive(Debug, Clone)]
    pub struct RedisCache {
        client: redis::aio::ConnectionManager,
        stats: std::sync::Arc<std::sync::atomic::AtomicU64>,
    }

    impl RedisCache {
        /// 从连接 URL 创建 Redis 缓存.
        pub async fn connect(url: &str) -> LsResult<Self> {
            let client = redis::Client::open(url)
                .map_err(|e| LsError::Internal(format!("redis connect: {e}")))?;
            let conn = redis::aio::ConnectionManager::new(client)
                .await
                .map_err(|e| LsError::Internal(format!("redis connection manager: {e}")))?;
            Ok(Self {
                client: conn,
                stats: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            })
        }
    }

    #[async_trait]
    impl CacheBackend for RedisCache {
        async fn get_raw(&self, key: &str) -> LsResult<Option<Vec<u8>>> {
            let mut conn = self.client.clone();
            let result: Option<Vec<u8>> = conn
                .get(key)
                .await
                .map_err(|e| LsError::Internal(format!("redis get: {e}")))?;
            self.stats
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(result)
        }

        async fn set_raw(&self, key: &str, value: &[u8], ttl_secs: u64) -> LsResult<()> {
            let mut conn = self.client.clone();
            let _: () = conn
                .set_ex(key, value, ttl_secs as usize)
                .await
                .map_err(|e| LsError::Internal(format!("redis set: {e}")))?;
            Ok(())
        }

        async fn delete(&self, key: &str) -> LsResult<bool> {
            let mut conn = self.client.clone();
            let result: i32 = conn
                .del(key)
                .await
                .map_err(|e| LsError::Internal(format!("redis del: {e}")))?;
            Ok(result > 0)
        }

        async fn exists(&self, key: &str) -> LsResult<bool> {
            let mut conn = self.client.clone();
            let result: bool = conn
                .exists(key)
                .await
                .map_err(|e| LsError::Internal(format!("redis exists: {e}")))?;
            Ok(result)
        }

        async fn clear(&self) -> LsResult<()> {
            let mut conn = self.client.clone();
            let _: () = redis::cmd("FLUSHDB")
                .query_async(&mut conn)
                .await
                .map_err(|e| LsError::Internal(format!("redis flushdb: {e}")))?;
            Ok(())
        }

        fn stats(&self) -> CacheStats {
            CacheStats {
                hits: self.stats.load(std::sync::atomic::Ordering::Relaxed),
                misses: 0,
                keys: 0,
                backend: "redis",
            }
        }
    }
}

// ── Memcached Backend ─────────────────────────────────────────────

#[cfg(feature = "memcached-cache")]
pub mod memcached_backend {
    use super::*;
    use memcache::Client;

    /// Memcached 缓存后端.
    #[derive(Debug, Clone)]
    pub struct MemcachedCache {
        client: Client,
        stats: std::sync::Arc<std::sync::atomic::AtomicU64>,
    }

    impl MemcachedCache {
        /// 从连接地址创建 Memcached 缓存.
        pub fn connect(addr: &str) -> LsResult<Self> {
            let client = Client::connect(addr)
                .map_err(|e| LsError::Internal(format!("memcached connect: {e}")))?;
            Ok(Self {
                client,
                stats: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            })
        }
    }

    #[async_trait]
    impl CacheBackend for MemcachedCache {
        async fn get_raw(&self, key: &str) -> LsResult<Option<Vec<u8>>> {
            let result: Option<Vec<u8>> = self
                .client
                .get(key)
                .map_err(|e| LsError::Internal(format!("memcached get: {e}")))?;
            self.stats
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(result)
        }

        async fn set_raw(&self, key: &str, value: &[u8], ttl_secs: u64) -> LsResult<()> {
            self.client
                .set(key, value, ttl_secs as u32)
                .map_err(|e| LsError::Internal(format!("memcached set: {e}")))?;
            Ok(())
        }

        async fn delete(&self, key: &str) -> LsResult<bool> {
            self.client
                .delete(key)
                .map_err(|e| LsError::Internal(format!("memcached del: {e}")))?;
            Ok(true)
        }

        async fn exists(&self, key: &str) -> LsResult<bool> {
            let result: Option<Vec<u8>> = self
                .client
                .get(key)
                .map_err(|e| LsError::Internal(format!("memcached exists: {e}")))?;
            Ok(result.is_some())
        }

        async fn clear(&self) -> LsResult<()> {
            self.client
                .flush()
                .map_err(|e| LsError::Internal(format!("memcached flush: {e}")))?;
            Ok(())
        }

        fn stats(&self) -> CacheStats {
            CacheStats {
                hits: self.stats.load(std::sync::atomic::Ordering::Relaxed),
                misses: 0,
                keys: 0,
                backend: "memcached",
            }
        }
    }
}

// ── String helper ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_cache() {
        let cache = CacheLayer::in_memory();
        cache.set("hello", &"world", 60).await.unwrap();
        let val: Option<String> = cache.get("hello").await.unwrap();
        assert_eq!(val, Some("world".into()));

        assert!(cache.exists("hello").await.unwrap());
        cache.delete("hello").await.unwrap();
        assert!(!cache.exists("hello").await.unwrap());
    }

    #[tokio::test]
    async fn test_cache_with_prefix() {
        let cache = CacheLayer::in_memory().with_prefix("tenant1");
        cache.set("key", &"value", 60).await.unwrap();
        let val: Option<String> = cache.get("key").await.unwrap();
        assert_eq!(val, Some("value".into()));
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = CacheLayer::in_memory();
        cache.set("a", &1, 60).await.unwrap();
        cache.set("b", &2, 60).await.unwrap();
        cache.clear().await.unwrap();
        let val: Option<i32> = cache.get("a").await.unwrap();
        assert!(val.is_none());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = CacheLayer::in_memory();
        let stats = cache.stats();
        assert_eq!(stats.backend, "in-memory");
    }

    #[test]
    fn test_llm_cache_key() {
        let key = llm_cache_key("gpt-4", "abc123", 70);
        assert_eq!(key, "llm:gpt-4:abc123:t70");
    }

    #[test]
    fn test_hash_messages() {
        let msgs = vec!["hello", "world"];
        let hash = hash_messages(&msgs);
        assert_eq!(hash.len(), 64); // SHA-256 hex
    }

    #[tokio::test]
    async fn test_llm_cache_wrapper() {
        let cache = CacheLayer::in_memory();
        let llm_cache = LlmCache::new(cache);
        assert!(llm_cache.enabled);

        let disabled = LlmCache::disabled();
        assert!(!disabled.enabled);
    }

    #[cfg(feature = "redis-cache")]
    #[tokio::test]
    async fn test_redis_cache_connect() {
        // Only runs if Redis is available
        let result = redis_backend::RedisCache::connect("redis://127.0.0.1:6379").await;
        // May fail if Redis is not running, but shouldn't panic
        if let Ok(cache) = result {
            let backend: Arc<dyn CacheBackend> = Arc::new(cache);
            let layer = CacheLayer::new(backend);
            layer.set("test", &"redis-ok", 10).await.unwrap();
        }
    }
}
