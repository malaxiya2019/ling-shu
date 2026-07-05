//! VectorMemory — Long-term vector storage interface
//!
//! Defines the trait for vector-based semantic memory retrieval.
//! Actual implementation uses the storage crate for persistence.

use async_trait::async_trait;
use crate::types::{MemoryItem, MemoryQuery};
use lingshu_core::LsResult;

/// A vector embedding with associated memory item
#[derive(Debug, Clone)]
pub struct VectorRecord {
    pub item: MemoryItem,
    pub embedding: Vec<f64>,
    pub score: Option<f64>,
}

/// Long-term vector memory storage trait
#[async_trait]
pub trait VectorMemory: Send + Sync {
    /// Store a memory item with its embedding vector
    async fn store(&self, item: MemoryItem, embedding: Vec<f64>) -> LsResult<String>;

    /// Search for similar memories by query embedding
    async fn search(&self, query: &MemoryQuery, embedding: Vec<f64>, top_k: usize) -> LsResult<Vec<VectorRecord>>;

    /// Search by text query (if embedding model is internal)
    async fn search_by_text(&self, query: &MemoryQuery, top_k: usize) -> LsResult<Vec<VectorRecord>>;

    /// Get memory by ID
    async fn get(&self, id: &str) -> LsResult<Option<MemoryItem>>;

    /// Delete a memory
    async fn delete(&self, id: &str) -> LsResult<bool>;

    /// Delete all memories for a session
    async fn clear_session(&self, session_id: &str) -> LsResult<usize>;
}

// ── In-memory implementation for testing ────────────


use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory vector store (for testing / development)
pub struct InMemoryVectorStore {
    records: Arc<RwLock<Vec<VectorRecord>>>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VectorMemory for InMemoryVectorStore {
    async fn store(&self, item: MemoryItem, embedding: Vec<f64>) -> LsResult<String> {
        let id = item.id.clone();
        self.records.write().await.push(VectorRecord {
            item,
            embedding,
            score: None,
        });
        Ok(id)
    }

    async fn search(&self, _query: &MemoryQuery, embedding: Vec<f64>, top_k: usize) -> LsResult<Vec<VectorRecord>> {
        let records = self.records.read().await;
        let mut scored: Vec<VectorRecord> = records
            .iter()
            .map(|r| {
                let score = cosine_similarity(&embedding, &r.embedding);
                let mut rec = r.clone();
                rec.score = Some(score);
                rec
            })
            .collect();
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        Ok(scored)
    }

    async fn search_by_text(&self, query: &MemoryQuery, top_k: usize) -> LsResult<Vec<VectorRecord>> {
        // Simple keyword-based fallback when no embedding model available
        let records = self.records.read().await;
        let query_lower = query.query.as_deref().unwrap_or("").to_lowercase();
        let mut scored: Vec<VectorRecord> = records
            .iter()
            .filter(|r| query_lower.is_empty() || r.item.content.to_lowercase().contains(&query_lower))
            .map(|r| {
                let mut rec = r.clone();
                rec.score = Some(if query_lower.is_empty() { 0.0 } else { 0.5 });
                rec
            })
            .collect();
        scored.truncate(top_k);
        Ok(scored)
    }

    async fn get(&self, id: &str) -> LsResult<Option<MemoryItem>> {
        let records = self.records.read().await;
        Ok(records.iter().find(|r| r.item.id == id).map(|r| r.item.clone()))
    }

    async fn delete(&self, id: &str) -> LsResult<bool> {
        let mut records = self.records.write().await;
        let len_before = records.len();
        records.retain(|r| r.item.id != id);
        Ok(records.len() < len_before)
    }

    async fn clear_session(&self, session_id: &str) -> LsResult<usize> {
        let mut records = self.records.write().await;
        let len_before = records.len();
        records.retain(|r| r.item.session_id != session_id);
        Ok(len_before - records.len())
    }
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MemoryItem;

    #[tokio::test]
    async fn test_store_and_search() {
        let store = InMemoryVectorStore::new();
        let item = MemoryItem::new("s1", "user", "hello world");
        let emb = vec![1.0, 0.0, 0.0];
        let id = store.store(item, emb).await.unwrap();

        let query = MemoryQuery {
            session_id: Some("s1".into()),
            ..Default::default()
        };
        let results = store.search(&query, vec![1.0, 0.0, 0.0], 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.id, id);
        assert!(results[0].score.unwrap() > 0.99);
    }

    #[tokio::test]
    async fn test_search_by_text() {
        let store = InMemoryVectorStore::new();
        store.store(MemoryItem::new("s1", "user", "apple banana"), vec![]).await.unwrap();
        store.store(MemoryItem::new("s1", "user", "cherry date"), vec![]).await.unwrap();

        let query = MemoryQuery {
            query: Some("apple".into()),
            ..Default::default()
        };
        let results = store.search_by_text(&query, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].item.content.contains("apple"));
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryVectorStore::new();
        let item = MemoryItem::new("s1", "user", "test");
        let id = store.store(item, vec![]).await.unwrap();
        assert!(store.delete(&id).await.unwrap());
        assert!(!store.delete(&id).await.unwrap());
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b)).abs() < 0.001);

        let c = vec![1.0, 2.0, 3.0];
        let d = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&c, &d) - 1.0).abs() < 0.001);
    }
}
