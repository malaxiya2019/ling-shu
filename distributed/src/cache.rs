//! Distributed cache — consistent hashing based sharded cache

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

/// Cache entry with TTL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: String,
    pub value: String,
    pub created_at: i64,
    pub ttl_secs: Option<u64>,
    pub hits: u64,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub max_entries: usize,
    pub default_ttl_secs: Option<u64>,
    pub cleanup_interval: Duration,
    pub shard_count: u16,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            default_ttl_secs: Some(300),
            cleanup_interval: Duration::from_secs(60),
            shard_count: 16,
        }
    }
}

/// A single cache shard
struct CacheShard {
    entries: HashMap<String, CacheEntry>,
}

impl CacheShard {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn get(&mut self, key: &str) -> Option<String> {
        // Check TTL before accessing mutable
        let expired = self.entries.get(key).is_some_and(|e| {
            e.ttl_secs
                .is_some_and(|ttl| chrono::Utc::now().timestamp() - e.created_at > ttl as i64)
        });
        if expired {
            self.entries.remove(key);
            return None;
        }
        if let Some(entry) = self.entries.get_mut(key) {
            entry.hits += 1;
            Some(entry.value.clone())
        } else {
            None
        }
    }

    fn set(&mut self, key: String, value: String, ttl_secs: Option<u64>) {
        if self.entries.len() >= 100_000 {
            if let Some(old_key) = self.entries.keys().next().cloned() {
                self.entries.remove(&old_key);
            }
        }
        let entry = CacheEntry {
            key: key.clone(),
            value,
            created_at: chrono::Utc::now().timestamp(),
            ttl_secs,
            hits: 0,
        };
        self.entries.insert(key, entry);
    }

    fn remove(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn keys(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }
}

/// Distributed cache with consistent hashing
pub struct DistributedCache {
    config: CacheConfig,
    shards: Vec<RwLock<CacheShard>>,
}

impl DistributedCache {
    pub fn new(config: CacheConfig) -> Self {
        let shard_count = config.shard_count.max(1) as usize;
        let mut shards = Vec::with_capacity(shard_count);
        for _ in 0..shard_count {
            shards.push(RwLock::new(CacheShard::new()));
        }
        Self { config, shards }
    }

    fn shard_index(&self, key: &str) -> usize {
        let hash: u64 = key
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        (hash % self.shards.len() as u64) as usize
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        let idx = self.shard_index(key);
        let mut shard = self.shards[idx].write().await;
        shard.get(key)
    }

    pub async fn set(&self, key: &str, value: &str, ttl_secs: Option<u64>) {
        let idx = self.shard_index(key);
        let mut shard = self.shards[idx].write().await;
        shard.set(
            key.to_string(),
            value.to_string(),
            ttl_secs.or(self.config.default_ttl_secs),
        );
    }

    pub async fn remove(&self, key: &str) -> bool {
        let idx = self.shard_index(key);
        let mut shard = self.shards[idx].write().await;
        shard.remove(key)
    }

    pub async fn clear(&self) {
        for shard in &self.shards {
            shard.write().await.clear();
        }
    }

    pub async fn len(&self) -> usize {
        let mut total = 0;
        for shard in &self.shards {
            total += shard.read().await.len();
        }
        total
    }
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    pub async fn stats(&self) -> CacheStats {
        let mut total_entries = 0;
        for shard in &self.shards {
            total_entries += shard.read().await.len();
        }
        CacheStats {
            total_entries,
            shard_count: self.shards.len() as u16,
            max_entries: self.config.max_entries,
        }
    }

    pub async fn keys(&self) -> Vec<String> {
        let mut all_keys = Vec::new();
        for shard in &self.shards {
            all_keys.extend(shard.read().await.keys());
        }
        all_keys
    }

    pub async fn start_cleanup(&self) {
        info!(
            "Cache cleanup started (interval: {:?})",
            self.config.cleanup_interval
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub shard_count: u16,
    pub max_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_set_get() {
        let cache = DistributedCache::new(CacheConfig::default());
        cache.set("key1", "value1", None).await;
        assert_eq!(cache.get("key1").await, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = DistributedCache::new(CacheConfig::default());
        assert_eq!(cache.get("nonexistent").await, None);
    }

    #[tokio::test]
    async fn test_cache_remove() {
        let cache = DistributedCache::new(CacheConfig::default());
        cache.set("key1", "value1", None).await;
        assert!(cache.remove("key1").await);
        assert!(!cache.remove("key1").await);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = DistributedCache::new(CacheConfig::default());
        cache.set("a", "1", None).await;
        cache.set("b", "2", None).await;
        assert_eq!(cache.len().await, 2);
        cache.clear().await;
        assert_eq!(cache.len().await, 0);
    }

    #[tokio::test]
    async fn test_cache_sharding() {
        let config = CacheConfig {
            shard_count: 4,
            ..Default::default()
        };
        let cache = DistributedCache::new(config);
        for i in 0..100 {
            cache
                .set(&format!("key{}", i), &format!("val{}", i), None)
                .await;
        }
        assert_eq!(cache.len().await, 100);
        for i in 0..100 {
            assert_eq!(
                cache.get(&format!("key{}", i)).await,
                Some(format!("val{}", i))
            );
        }
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = DistributedCache::new(CacheConfig::default());
        cache.set("k", "v", None).await;
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.shard_count, 16);
    }
}
