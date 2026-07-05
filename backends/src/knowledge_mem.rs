//! InMemoryKnowledge — 基于内存的轻量知识库实现.
//!
//! 提供完整 `Knowledge` trait 实现，数据驻留内存。
//! 适用于开发/测试环境，或轻量级单机部署。

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::knowledge::{DataSource, Knowledge, KnowledgeEntry, KnowledgeSearchResult};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// 内存知识库.
pub struct InMemoryKnowledge {
    sources: RwLock<HashMap<LsId, DataSource>>,
    entries: RwLock<HashMap<LsId, KnowledgeEntry>>,
    entry_versions: RwLock<HashMap<LsId, Vec<KnowledgeEntry>>>,
    source_entries: RwLock<HashMap<LsId, Vec<LsId>>>,
}

impl Default for InMemoryKnowledge {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryKnowledge {
    pub fn new() -> Self {
        Self {
            sources: RwLock::new(HashMap::new()),
            entries: RwLock::new(HashMap::new()),
            entry_versions: RwLock::new(HashMap::new()),
            source_entries: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl Knowledge for InMemoryKnowledge {
    async fn register_source(&self, _ctx: LsContext, source: DataSource) -> LsResult<LsId> {
        let id = source.source_id;
        let mut sources = self.sources.write().await;
        if sources.contains_key(&id) {
            return Err(LsError::AlreadyExists(format!(
                "knowledge source {} already registered",
                id
            )));
        }
        sources.insert(id, source);
        Ok(id)
    }

    async fn unregister_source(&self, _ctx: LsContext, source_id: LsId) -> LsResult<()> {
        let mut sources = self.sources.write().await;
        sources
            .remove(&source_id)
            .ok_or_else(|| LsError::NotFound(format!("knowledge source {source_id} not found")))?;

        // Remove all entries for this source
        let mut source_entries = self.source_entries.write().await;
        if let Some(entry_ids) = source_entries.remove(&source_id) {
            let mut entries = self.entries.write().await;
            let mut versions = self.entry_versions.write().await;
            for eid in entry_ids {
                entries.remove(&eid);
                versions.remove(&eid);
            }
        }
        Ok(())
    }

    async fn sync(&self, _ctx: LsContext, source_id: LsId) -> LsResult<u64> {
        let sources = self.sources.read().await;
        if !sources.contains_key(&source_id) {
            return Err(LsError::NotFound(format!(
                "knowledge source {source_id} not found"
            )));
        }
        // In-memory: sync is a no-op (data is already in memory)
        let count = self
            .source_entries
            .read()
            .await
            .get(&source_id)
            .map(|v| v.len() as u64)
            .unwrap_or(0);
        Ok(count)
    }

    async fn search(
        &self,
        _ctx: LsContext,
        query: &str,
        limit: u64,
    ) -> LsResult<KnowledgeSearchResult> {
        let entries = self.entries.read().await;
        let query_lower = query.to_lowercase();

        let mut results: Vec<KnowledgeEntry> = entries
            .values()
            .filter(|e| {
                let content_str = match &e.content {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                content_str.to_lowercase().contains(&query_lower)
                    || e.metadata
                        .values()
                        .any(|v| v.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect();

        let total = results.len() as u64;
        results.truncate(limit as usize);

        Ok(KnowledgeSearchResult {
            entries: results,
            total,
        })
    }

    async fn get_entry(&self, _ctx: LsContext, entry_id: LsId) -> LsResult<KnowledgeEntry> {
        let entries = self.entries.read().await;
        entries
            .get(&entry_id)
            .cloned()
            .ok_or_else(|| LsError::NotFound(format!("knowledge entry {entry_id} not found")))
    }

    async fn get_entry_history(
        &self,
        _ctx: LsContext,
        entry_id: LsId,
    ) -> LsResult<Vec<KnowledgeEntry>> {
        let versions = self.entry_versions.read().await;
        versions
            .get(&entry_id)
            .cloned()
            .ok_or_else(|| LsError::NotFound(format!("knowledge entry {entry_id} has no history")))
    }
}

impl InMemoryKnowledge {
    /// 直接插入一条知识条目 (用于测试/初始化).
    pub async fn insert_entry(&self, entry: KnowledgeEntry) -> LsResult<()> {
        let id = entry.entry_id;
        let _source_id = entry.source.parse::<String>().unwrap_or_default();
        let source_lsid = LsId::new();

        // Update entry
        self.entries.write().await.insert(id, entry.clone());

        // Track version history
        let mut versions = self.entry_versions.write().await;
        versions.entry(id).or_default().push(entry);

        // Track source→entry mapping
        self.source_entries
            .write()
            .await
            .entry(source_lsid)
            .or_default()
            .push(id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_entry(content: &str, source: &str) -> KnowledgeEntry {
        KnowledgeEntry {
            entry_id: LsId::new(),
            source: source.to_string(),
            content: Value::String(content.to_string()),
            version: 1,
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_register_source() {
        let kb = InMemoryKnowledge::new();
        let ctx = LsContext::with_session(LsId::new());
        let source = DataSource {
            source_id: LsId::new(),
            name: "docs".into(),
            source_type: "manual".into(),
            config: Value::Null,
        };
        let id = kb.register_source(ctx.clone(), source).await.unwrap();
        assert!(id.to_string().len() > 0);
    }

    #[tokio::test]
    async fn test_search() {
        let kb = InMemoryKnowledge::new();
        let ctx = LsContext::with_session(LsId::new());

        let entry = test_entry("Rust is a systems programming language", "rust-docs");
        kb.insert_entry(entry).await.unwrap();

        let result = kb.search(ctx, "Rust", 10).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.entries[0].source, "rust-docs");
    }

    #[tokio::test]
    async fn test_search_no_match() {
        let kb = InMemoryKnowledge::new();
        let ctx = LsContext::with_session(LsId::new());

        let entry = test_entry("Python is great", "python-docs");
        kb.insert_entry(entry).await.unwrap();

        let result = kb.search(ctx, "Rust", 10).await.unwrap();
        assert_eq!(result.total, 0);
    }

    #[tokio::test]
    async fn test_get_entry_not_found() {
        let kb = InMemoryKnowledge::new();
        let ctx = LsContext::with_session(LsId::new());
        let result = kb.get_entry(ctx, LsId::new()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
