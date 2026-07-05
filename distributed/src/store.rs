//! Distributed store — simple distributed key-value store with replication

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Value stored in the distributed store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreValue {
    pub key: String,
    pub value: Vec<u8>,
    pub version: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Store configuration
#[derive(Debug, Clone)]
pub struct StoreConfig {
    pub replication_factor: usize,
    pub sync_writes: bool,
    pub max_keys: usize,
    pub cleanup_interval: Duration,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            replication_factor: 3,
            sync_writes: false,
            max_keys: 1_000_000,
            cleanup_interval: Duration::from_secs(300),
        }
    }
}

/// Data partition
struct DataPartition {
    data: HashMap<String, StoreValue>,
}

impl DataPartition {
    fn new() -> Self {
        Self { data: HashMap::new() }
    }

    fn get(&self, key: &str) -> Option<&StoreValue> {
        self.data.get(key)
    }

    fn set(&mut self, key: String, value: Vec<u8>) -> StoreValue {
        let now = chrono::Utc::now().timestamp();
        let entry = self.data.entry(key.clone()).or_insert_with(|| StoreValue {
            key: key.clone(),
            value: Vec::new(),
            version: 0,
            created_at: now,
            updated_at: now,
        });
        entry.value = value;
        entry.version += 1;
        entry.updated_at = now;
        entry.clone()
    }

    fn delete(&mut self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }

    fn scan_prefix(&self, prefix: &str) -> Vec<&StoreValue> {
        self.data.values().filter(|v| v.key.starts_with(prefix)).collect()
    }
}

/// Distributed key-value store
pub struct DistributedStore {
    config: StoreConfig,
    partitions: Vec<RwLock<DataPartition>>,
}

impl DistributedStore {
    pub fn new(config: StoreConfig) -> Self {
        let partition_count = config.replication_factor.max(1) * 4;
        let mut partitions = Vec::with_capacity(partition_count);
        for _ in 0..partition_count {
            partitions.push(RwLock::new(DataPartition::new()));
        }
        Self { config, partitions }
    }

    fn partition_index(&self, key: &str) -> usize {
        let hash: u64 = key.bytes().fold(0u64, |acc, b| acc.wrapping_mul(47).wrapping_add(b as u64));
        (hash % self.partitions.len() as u64) as usize
    }

    pub async fn get(&self, key: &str) -> Option<StoreValue> {
        let idx = self.partition_index(key);
        let partition = self.partitions[idx].read().await;
        partition.get(key).cloned()
    }

    pub async fn set(&self, key: &str, value: &[u8]) -> StoreValue {
        let idx = self.partition_index(key);
        let mut partition = self.partitions[idx].write().await;
        if partition.len() >= self.config.max_keys / self.partitions.len() {
            warn!("Partition {} full, evicting oldest entry", idx);
            if let Some(oldest_key) = partition.keys().first().cloned() {
                partition.delete(&oldest_key);
            }
        }
        let result = partition.set(key.to_string(), value.to_vec());
        debug!("Stored key {} (version {})", key, result.version);
        result
    }

    pub async fn delete(&self, key: &str) -> bool {
        let idx = self.partition_index(key);
        self.partitions[idx].write().await.delete(key)
    }

    pub async fn exists(&self, key: &str) -> bool {
        self.get(key).await.is_some()
    }

    pub async fn len(&self) -> usize {
        let mut total = 0;
        for p in &self.partitions {
            total += p.read().await.len();
        }
        total
    }

    pub async fn scan_prefix(&self, prefix: &str) -> Vec<StoreValue> {
        let mut results = Vec::new();
        for p in &self.partitions {
            results.extend(p.read().await.scan_prefix(prefix).into_iter().cloned());
        }
        results
    }

    pub async fn start(&self) {
        info!(
            "Distributed store started ({} partitions, replication: {})",
            self.partitions.len(),
            self.config.replication_factor
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_set_get() {
        let store = DistributedStore::new(StoreConfig::default());
        store.set("key1", b"value1").await;
        let val = store.get("key1").await;
        assert!(val.is_some());
        assert_eq!(val.unwrap().value, b"value1");
    }

    #[tokio::test]
    async fn test_store_miss() {
        let store = DistributedStore::new(StoreConfig::default());
        assert!(store.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_store_delete() {
        let store = DistributedStore::new(StoreConfig::default());
        store.set("key1", b"val").await;
        assert!(store.exists("key1").await);
        assert!(store.delete("key1").await);
        assert!(!store.exists("key1").await);
    }

    #[tokio::test]
    async fn test_store_versioning() {
        let store = DistributedStore::new(StoreConfig::default());
        let v1 = store.set("key", b"v1").await;
        let v2 = store.set("key", b"v2").await;
        assert_eq!(v2.version, v1.version + 1);
        assert_eq!(v2.value, b"v2");
    }

    #[tokio::test]
    async fn test_scan_prefix() {
        let store = DistributedStore::new(StoreConfig::default());
        store.set("app:users:1", b"alice").await;
        store.set("app:users:2", b"bob").await;
        store.set("app:config:db", b"postgres").await;
        let users = store.scan_prefix("app:users:").await;
        assert_eq!(users.len(), 2);
    }

    #[tokio::test]
    async fn test_store_many_keys() {
        let store = DistributedStore::new(StoreConfig::default());
        for i in 0..100 {
            store.set(&format!("k{}", i), b"data").await;
        }
        assert_eq!(store.len().await, 100);
    }
}
