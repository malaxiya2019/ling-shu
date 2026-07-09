//! In-Memory 缓存后端 — 基于 `HashMap` + `tokio::sync::RwLock`.
//!
//! 用于开发和测试，生产环境建议使用 Redis 或 Memcached 后端。

use async_trait::async_trait;
use lingshu_core::LsResult;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::CacheBackend;
use crate::CacheStats;

/// 内存缓存条目.
#[derive(Debug, Clone)]
struct CacheEntry {
    data: Vec<u8>,
    expires_at: Option<Instant>,
}

/// In-Memory 缓存后端.
#[derive(Debug, Clone)]
pub struct InMemoryCache {
    store: Arc<RwLock<HashMap<String, CacheEntry>>>,
    hits: Arc<AtomicU64>,
    misses: Arc<AtomicU64>,
}

impl InMemoryCache {
    /// 创建新的内存缓存.
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 创建预分配容量的内存缓存.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::with_capacity(capacity))),
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 清理过期条目.
    pub async fn purge_expired(&self) -> LsResult<usize> {
        let mut store = self.store.write().await;
        let before = store.len();
        store.retain(|_, entry| match entry.expires_at {
            Some(expiry) => expiry > Instant::now(),
            None => true,
        });
        Ok(before - store.len())
    }
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CacheBackend for InMemoryCache {
    async fn get_raw(&self, key: &str) -> LsResult<Option<Vec<u8>>> {
        let store = self.store.read().await;
        match store.get(key) {
            Some(entry) => {
                // 检查是否过期
                if let Some(expires_at) = entry.expires_at {
                    if expires_at <= Instant::now() {
                        drop(store);
                        self.delete(key).await?;
                        self.misses.fetch_add(1, Ordering::Relaxed);
                        return Ok(None);
                    }
                }
                self.hits.fetch_add(1, Ordering::Relaxed);
                Ok(Some(entry.data.clone()))
            }
            None => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
        }
    }

    async fn set_raw(&self, key: &str, value: &[u8], ttl_secs: u64) -> LsResult<()> {
        let mut store = self.store.write().await;
        let expires_at = if ttl_secs > 0 {
            Some(Instant::now() + Duration::from_secs(ttl_secs))
        } else {
            None
        };
        store.insert(
            key.to_string(),
            CacheEntry {
                data: value.to_vec(),
                expires_at,
            },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> LsResult<bool> {
        let mut store = self.store.write().await;
        let existed = store.remove(key).is_some();
        Ok(existed)
    }

    async fn exists(&self, key: &str) -> LsResult<bool> {
        let store = self.store.read().await;
        match store.get(key) {
            Some(entry) => {
                if let Some(expires_at) = entry.expires_at {
                    if expires_at <= Instant::now() {
                        drop(store);
                        self.delete(key).await?;
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn clear(&self) -> LsResult<()> {
        let mut store = self.store.write().await;
        store.clear();
        Ok(())
    }

    fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            keys: 0, // Would need a read lock
            backend: "in-memory",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_get() {
        let cache = InMemoryCache::new();
        cache.set_raw("key1", b"value1", 60).await.unwrap();
        let result = cache.get_raw("key1").await.unwrap();
        assert_eq!(result, Some(b"value1".to_vec()));
    }

    #[tokio::test]
    async fn test_missing_key() {
        let cache = InMemoryCache::new();
        let result = cache.get_raw("nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_ttl_expiry() {
        let cache = InMemoryCache::new();
        cache.set_raw("temp", b"data", 1).await.unwrap(); // 1 sec TTL
        assert!(cache.exists("temp").await.unwrap());
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert!(!cache.exists("temp").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let cache = InMemoryCache::new();
        cache.set_raw("delkey", b"val", 60).await.unwrap();
        assert!(cache.delete("delkey").await.unwrap());
        assert!(!cache.delete("delkey").await.unwrap());
    }

    #[tokio::test]
    async fn test_clear() {
        let cache = InMemoryCache::new();
        cache.set_raw("a", b"1", 60).await.unwrap();
        cache.set_raw("b", b"2", 60).await.unwrap();
        cache.clear().await.unwrap();
        assert!(!cache.exists("a").await.unwrap());
        assert!(!cache.exists("b").await.unwrap());
    }

    #[tokio::test]
    async fn test_purge_expired() {
        let cache = InMemoryCache::new();
        cache.set_raw("expire_soon", b"data", 1).await.unwrap();
        cache.set_raw("keep", b"data", 3600).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let purged = cache.purge_expired().await.unwrap();
        assert_eq!(purged, 1);
        assert!(!cache.exists("expire_soon").await.unwrap());
        assert!(cache.exists("keep").await.unwrap());
    }

    #[test]
    fn test_stats() {
        let cache = InMemoryCache::new();
        let stats = cache.stats();
        assert_eq!(stats.backend, "in-memory");
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }
}
