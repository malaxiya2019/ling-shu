//! 🧠 RAG Plugin — Retrieval-Augmented Generation for Lingshu.
//!
//! 提供文档分块、简易词频嵌入和余弦相似度检索能力。
//! 零外部 AI 依赖，纯 Rust 实现，适合 Termux / aarch64 环境。
//!
//! ## 用法
//!
//! ```ignore
//! use lingshu_rag_plugin::RagPlugin;
//!
//! let plugin = RagPlugin::default();
//! plugin.store_document("Rust is a systems language empowering everyone.").await?;
//! let results = plugin.search("systems language", 3).await?;
//! let ctx = plugin.query_for_llm("systems language", 3).await;
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::plugin::{Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus};
use text_splitter::TextSplitter;

// ---------------------------------------------------------------------------
// 文本分块器
// ---------------------------------------------------------------------------

/// 基于 `text-splitter` 的分块器，按字符容量切割。
struct TextChunker {
    splitter: TextSplitter<text_splitter::Characters>,
}

impl TextChunker {
    fn new(max_chars: usize) -> Self {
        Self {
            splitter: TextSplitter::new(max_chars),
        }
    }

    /// 将长文本切割为多个块。
    fn chunk<'a>(&self, text: &'a str) -> Vec<&'a str> {
        self.splitter.chunks(text).collect()
    }
}

// ---------------------------------------------------------------------------
// 简易嵌入器（词频袋）
// ---------------------------------------------------------------------------

/// 简易词频嵌入器。
///
/// 将文本分词、小写化，映射到一个固定维度的稀疏向量。
/// 维度通过词项的确定性哈希映射，无需外部模型。
struct SimpleEmbedder {
    /// 嵌入向量维度
    dim: usize,
}

impl SimpleEmbedder {
    fn new(dim: usize) -> Self {
        Self { dim }
    }

    /// 将文本转换为词频向量。
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0_f32; self.dim];

        for token in text.split_whitespace() {
            let cleaned: String = token
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(|c| c.to_lowercase())
                .collect();

            if cleaned.is_empty() {
                continue;
            }

            let idx = self.hash_to_index(&cleaned);
            vec[idx] += 1.0;
        }

        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }

        vec
    }

    fn hash_to_index(&self, token: &str) -> usize {
        let mut h: usize = 5381;
        for b in token.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as usize);
        }
        h % self.dim
    }
}

// ---------------------------------------------------------------------------
// 存储层
// ---------------------------------------------------------------------------

struct DocumentStore {
    #[allow(clippy::type_complexity)]
    docs: RwLock<HashMap<String, Vec<(String, Vec<f32>)>>>,
}

impl DocumentStore {
    fn new() -> Self {
        Self {
            docs: RwLock::new(HashMap::new()),
        }
    }

    fn store(&self, doc_id: String, chunks: Vec<(String, Vec<f32>)>) {
        let mut map = self.docs.write().expect("RwLock poisoned");
        map.insert(doc_id, chunks);
    }

    fn search(&self, query_embed: &[f32], top_k: usize) -> Vec<(String, String, f32)> {
        let map = self.docs.read().expect("RwLock poisoned");
        let mut scored: Vec<(String, String, f32)> = Vec::new();

        for (doc_id, chunks) in map.iter() {
            for (chunk_text, chunk_embed) in chunks {
                let sim = cosine_similarity(query_embed, chunk_embed);
                scored.push((doc_id.clone(), chunk_text.clone(), sim));
            }
        }

        scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }

    fn list_documents(&self) -> Vec<String> {
        let map = self.docs.read().expect("RwLock poisoned");
        map.keys().cloned().collect()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|v| v * v).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// ---------------------------------------------------------------------------
// RAG 插件
// ---------------------------------------------------------------------------

/// RAG 插件 —— 文档分块、嵌入、检索。
pub struct RagPlugin {
    info: PluginInfo,
    chunker: TextChunker,
    embedder: SimpleEmbedder,
    store: DocumentStore,
}

impl Default for RagPlugin {
    fn default() -> Self {
        Self::new(256, 128)
    }
}

impl RagPlugin {
    /// 创建 RAG 插件实例。
    pub fn new(max_chars_per_chunk: usize, embedding_dim: usize) -> Self {
        let manifest = PluginManifest {
            name: "rag-plugin".into(),
            version: "1.0.0".into(),
            description: "Retrieval-Augmented Generation — 文档分块、嵌入、语义检索".into(),
            author: Some("Lingshu Team".into()),
            homepage: None,
            license: Some("MIT OR Apache-2.0".into()),
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![PluginPermission {
                resource: "memory".into(),
                actions: vec!["read".into(), "write".into()],
            }],
            min_api_version: Some("1.0.0".into()),
        ..Default::default()
        };

        let info = PluginInfo {
            plugin_id: LsId::new(),
            manifest,
            status: PluginStatus::Installed,
            loaded_at: None,
        };

        Self {
            info,
            chunker: TextChunker::new(max_chars_per_chunk),
            embedder: SimpleEmbedder::new(embedding_dim),
            store: DocumentStore::new(),
        }
    }

    /// 存储文档：分块 → 嵌入 → 存储。
    pub async fn store_document(&self, text: &str) -> String {
        let doc_id = uuid::Uuid::new_v4().to_string();
        let chunks: Vec<&str> = self.chunker.chunk(text);

        let embedded: Vec<(String, Vec<f32>)> = chunks
            .iter()
            .map(|chunk| {
                let emb = self.embedder.embed(chunk);
                (chunk.to_string(), emb)
            })
            .collect();

        let num_chunks = embedded.len();
        self.store.store(doc_id.clone(), embedded);

        tracing::info!(
            plugin = "rag-plugin",
            doc_id = %doc_id,
            chunks = num_chunks,
            "Document stored"
        );

        doc_id
    }

    /// 语义搜索：对查询进行嵌入，余弦相似度检索 top-k 个结果。
    pub async fn search(&self, query: &str, top_k: usize) -> Vec<(String, String, f32)> {
        let query_embed = self.embedder.embed(query);
        self.store.search(&query_embed, top_k)
    }

    /// 列出所有已存储的文档 ID。
    pub async fn list_documents(&self) -> Vec<String> {
        self.store.list_documents()
    }

    /// 获取存储的文档数量。
    pub async fn document_count(&self) -> usize {
        self.store.list_documents().len()
    }

    /// 获取统计信息。
    pub async fn stats(&self) -> serde_json::Value {
        let doc_ids = self.store.list_documents();
        let doc_count = doc_ids.len();
        let chunk_count: usize = doc_ids
            .iter()
            .filter_map(|id| {
                let map = self.store.docs.read().ok()?;
                map.get(id).map(|chunks| chunks.len())
            })
            .sum();

        serde_json::json!({
            "documents": doc_count,
            "chunks": chunk_count,
            "embedding_dim": self.embedder.dim,
        })
    }

    /// 搜索并格式化为 LLM 上下文（RAG 核心功能）.
    pub async fn query_for_llm(&self, query: &str, top_k: usize) -> String {
        let results = self.search(query, top_k).await;
        Self::format_context(&results)
    }

    /// 将搜索结果格式化为 LLM 友好的上下文文本.
    pub fn format_context(results: &[(String, String, f32)]) -> String {
        if results.is_empty() {
            return "无相关文档。".to_string();
        }

        let mut context = String::from("以下是与用户问题相关的文档片段（按相关性排序）：\n\n");
        for (i, (_doc_id, chunk_text, score)) in results.iter().enumerate() {
            let relevance = if *score > 0.7 {
                "【高度相关】"
            } else if *score > 0.4 {
                "【中度相关】"
            } else {
                "【低度相关】"
            };
            context.push_str(&format!(
                "{relevance} [文档 {i}/{}] (相关度: {:.2})\n{}\n\n",
                results.len(),
                score,
                chunk_text
            ));
        }

        context
    }

    /// 删除指定文档.
    pub async fn delete_document(&self, doc_id: &str) -> bool {
        let mut map = self.store.docs.write().expect("RwLock poisoned");
        map.remove(doc_id).is_some()
    }

    /// 清除所有文档.
    pub async fn clear(&self) {
        let mut map = self.store.docs.write().expect("RwLock poisoned");
        map.clear();
    }

    /// 获取指定文档的块数.
    pub async fn chunk_count_for(&self, doc_id: &str) -> Option<usize> {
        let map = self.store.docs.read().ok()?;
        map.get(doc_id).map(|chunks| chunks.len())
    }
}

// ---------------------------------------------------------------------------
// Plugin trait 实现
// ---------------------------------------------------------------------------

#[async_trait]
impl Plugin for RagPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "rag-plugin", "RAG plugin initialized");
        Ok(())
    }

    async fn start(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "rag-plugin", "RAG plugin started");
        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "rag-plugin", "RAG plugin stopped");
        Ok(())
    }

    fn required_permissions(&self) -> Vec<PluginPermission> {
        self.info.manifest.permissions.clone()
    }
}

// ---------------------------------------------------------------------------
// 动态加载入口
// ---------------------------------------------------------------------------

/// 创建 RAG 插件实例（用于动态加载）。
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedder_basic() {
        let emb = SimpleEmbedder::new(128);
        let v1 = emb.embed("hello world");
        let v2 = emb.embed("hello world");
        assert_eq!(v1.len(), 128);
        assert!((cosine_similarity(&v1, &v2) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedder_different_texts() {
        let emb = SimpleEmbedder::new(128);
        let v1 = emb.embed("apple banana");
        let v2 = emb.embed("dog cat");
        let sim = cosine_similarity(&v1, &v2);
        assert!(sim < 0.8, "Expected low similarity, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_store_and_search() {
        let plugin = RagPlugin::default();
        plugin
            .store_document("The Rust programming language is fast and memory-efficient")
            .await;
        plugin
            .store_document("Python is a dynamically typed interpreted language")
            .await;

        let results = plugin.search("programming language", 2).await;
        assert!(!results.is_empty(), "Expected at least one result");
        assert!(results[0].2 > 0.0, "Expected positive similarity score");
    }

    #[tokio::test]
    async fn test_list_documents() {
        let plugin = RagPlugin::default();
        plugin.store_document("Some content here").await;
        let docs = plugin.list_documents().await;
        assert_eq!(docs.len(), 1);
    }

    #[tokio::test]
    async fn test_stats() {
        let plugin = RagPlugin::default();
        plugin.store_document("Content A").await;
        plugin.store_document("Content B").await;
        let stats = plugin.stats().await;
        assert_eq!(stats["documents"].as_u64(), Some(2));
    }

    #[test]
    fn test_plugin_info() {
        let plugin = RagPlugin::default();
        let info = plugin.info();
        assert_eq!(info.manifest.name, "rag-plugin");
        assert_eq!(info.manifest.plugin_type, "static");
    }

    #[test]
    fn test_plugin_permissions() {
        let plugin = RagPlugin::default();
        let perms = plugin.required_permissions();
        assert!(perms.iter().any(|p| p.resource == "memory"));
    }

    #[test]
    fn test_chunker() {
        let chunker = TextChunker::new(50);
        let text = "word ".repeat(200);
        let chunks: Vec<&str> = chunker.chunk(&text);
        assert!(chunks.len() >= 2, "Expected multiple chunks");
    }

    // ── 新增功能测试 ──

    #[tokio::test]
    async fn test_query_for_llm() {
        let plugin = RagPlugin::default();
        plugin.store_document("Rust is a systems language focused on safety and performance").await;
        plugin.store_document("Python is an interpreted high-level programming language").await;

        let context = plugin.query_for_llm("Rust safety", 2).await;
        assert!(!context.is_empty(), "Expected non-empty context, got empty");
        assert!(context.contains("用户问题"), "Expected formatted context, got: {context}");
        assert!(context.contains("Rust"), "Expected Rust-related content, got: {context}");
    }

    #[tokio::test]
    async fn test_delete_document() {
        let plugin = RagPlugin::default();
        let doc_id = plugin.store_document("Test document").await;
        assert_eq!(plugin.document_count().await, 1);

        let deleted = plugin.delete_document(&doc_id).await;
        assert!(deleted, "Expected successful deletion");
        assert_eq!(plugin.document_count().await, 0);
    }

    #[tokio::test]
    async fn test_clear() {
        let plugin = RagPlugin::default();
        plugin.store_document("Doc A").await;
        plugin.store_document("Doc B").await;
        assert_eq!(plugin.document_count().await, 2);

        plugin.clear().await;
        assert_eq!(plugin.document_count().await, 0);
    }

    #[test]
    fn test_format_context_empty() {
        let results: Vec<(String, String, f32)> = vec![];
        let context = RagPlugin::format_context(&results);
        assert_eq!(context, "无相关文档。");
    }

    #[test]
    fn test_format_context_with_results() {
        let results = vec![
            ("doc1".to_string(), "Rust is safe".to_string(), 0.95),
            ("doc2".to_string(), "Python is flexible".to_string(), 0.45),
        ];
        let context = RagPlugin::format_context(&results);
        assert!(context.contains("高度相关"));
        assert!(context.contains("中度相关"));
        assert!(context.contains("Rust is safe"));
    }
}
