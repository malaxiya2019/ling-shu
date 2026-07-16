//! SemanticIndex — 语义索引 trait 与 TF-IDF 实现

use crate::tokenizer;
use async_trait::async_trait;
use lingshu_memory_episode::{Episode, EpisodeRepository, EpisodeQuery};
use std::collections::HashMap;
use tokio::sync::RwLock;

// ─── ScoredEpisode ──────────────────────────────────────

/// 带相关度评分的 Episode 搜索结果。
#[derive(Debug, Clone)]
pub struct ScoredEpisode {
    pub episode: Episode,
    /// 相关度评分 (0.0 ~ 1.0)
    pub score: f64,
    /// 命中的词项列表
    pub matched_terms: Vec<String>,
}

// ─── SemanticIndex trait ────────────────────────────────

/// 语义索引 — 抽象的向量/语义搜索接口。
///
/// 不绑定任何具体的 embedding 模型或向量数据库。
/// 当前内置实现：TfIdfIndex（纯本地，无外部依赖）。
#[async_trait]
pub trait SemanticIndex: Send + Sync {
    /// 索引名称。
    fn name(&self) -> &str;

    /// 搜索与查询最相关的 Episode。
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<ScoredEpisode>, SemanticError>;

    /// 索引单条 Episode。
    async fn index_episode(&self, episode: &Episode) -> Result<(), SemanticError>;

    /// 批量索引 Episode。
    async fn index_batch(&self, episodes: &[Episode]) -> Result<(), SemanticError>;

    /// 从 Episode Repository 重建整个索引。
    async fn rebuild_from_store(&self, store: &dyn EpisodeRepository) -> Result<usize, SemanticError>;

    /// 索引中的文档数。
    fn doc_count(&self) -> usize;

    /// 清空索引。
    async fn clear(&self) -> Result<(), SemanticError>;
}

// ─── TfIdfIndex ────────────────────────────────────────

/// TF-IDF 语义索引 — 纯本地实现，无需外部 Embedding API。
///
/// 使用 TF-IDF (Term Frequency - Inverse Document Frequency) 算法
/// 对 Episode 内容进行向量化，通过余弦相似度排序搜索结果。
///
/// 中文支持：使用字符 n-gram 分词（单字 + 双字组合）
fn idf(total_docs: usize, doc_freq: usize) -> f64 {
    if doc_freq == 0 {
        return 0.0;
    }
    ((total_docs as f64) / (doc_freq as f64)).ln() + 1.0
}

fn tf(term_freq: usize, total_terms: usize) -> f64 {
    if total_terms == 0 {
        return 0.0;
    }
    (term_freq as f64) / (total_terms as f64)
}

fn cosine_similarity(a: &[(String, f64)], b: &[(String, f64)]) -> f64 {
    let a_map: HashMap<&str, f64> = a.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let b_map: HashMap<&str, f64> = b.iter().map(|(k, v)| (k.as_str(), *v)).collect();

    let mut dot_product = 0.0;
    let mut a_norm = 0.0;
    let mut b_norm = 0.0;

    for (term, weight) in &a_map {
        a_norm += weight * weight;
        if let Some(b_weight) = b_map.get(term) {
            dot_product += weight * b_weight;
        }
    }
    for (_, weight) in &b_map {
        b_norm += weight * weight;
    }

    a_norm = a_norm.sqrt();
    b_norm = b_norm.sqrt();

    if a_norm == 0.0 || b_norm == 0.0 {
        return 0.0;
    }

    dot_product / (a_norm * b_norm)
}

struct IndexedDoc {
    episode: Episode,
    /// TF-IDF 向量: (词项, 权重)，f32 量化存储
    vector: Vec<(String, f32)>,
    /// 词项频率 (用于 TF 计算)
    term_freq: HashMap<String, usize>,
}

/// TF-IDF 索引实现。
pub struct TfIdfIndex {
    name: String,
    docs: RwLock<Vec<IndexedDoc>>,
    /// 文档频率: 每个词项出现在多少个文档中
    df: RwLock<HashMap<String, usize>>,
    /// 是否启用 f32 量化存储
    quantized: bool,
}

impl TfIdfIndex {
    pub fn new() -> Self {
        Self {
            name: "tfidf".to_string(),
            docs: RwLock::new(Vec::new()),
            df: RwLock::new(HashMap::new()),
            quantized: true,
        }
    }

    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            docs: RwLock::new(Vec::new()),
            df: RwLock::new(HashMap::new()),
            quantized: true,
        }
    }

    /// 计算单条 Episode 的 TF-IDF 向量（返回 f32 量化）。
    #[allow(dead_code)]
    fn compute_vector(&self, episode: &Episode, df: &HashMap<String, usize>, total_docs: usize) -> Vec<(String, f32)> {
        // 组合标题和摘要作为文本内容
        let text = format!("{} {}", episode.title, episode.summary);
        let tokens = tokenizer::tokenize(&text);
        let total_terms = tokens.len();

        if total_terms == 0 {
            return Vec::new();
        }

        // 计算词频
        let mut term_freq_map: HashMap<String, usize> = HashMap::new();
        for token in &tokens {
            *term_freq_map.entry(token.clone()).or_insert(0) += 1;
        }

        // 计算 TF-IDF
        let mut vector: Vec<(String, f64)> = Vec::new();
        for (term, freq) in &term_freq_map {
            let tf_val = tf(*freq, total_terms);
            let doc_freq = df.get(term).copied().unwrap_or(1);
            let idf_val = idf(total_docs, doc_freq);
            vector.push((term.clone(), tf_val * idf_val));
        }

        // 按权重降序排列
        vector.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        // 量化 f64→f32
        vector.into_iter().map(|(k, v)| (k, v as f32)).collect()
    }

    /// 合并向量（计算所有 Episode 的 DF）。
    fn build_df(episodes: &[Episode]) -> HashMap<String, usize> {
        let mut df: HashMap<String, usize> = HashMap::new();
        for ep in episodes {
            let text = format!("{} {}", ep.title, ep.summary);
            let unique: std::collections::HashSet<String> = tokenizer::unique_tokens(&text);
            for token in unique {
                *df.entry(token).or_insert(0) += 1;
            }
        }
        df
    }

    /// 启用或禁用 f32 量化存储。
    /// 启用时内存降 50%，计算时会自动反量化。
    pub fn set_quantized(&mut self, enabled: bool) {
        self.quantized = enabled;
    }

    /// 当前是否启用量化。
    pub fn is_quantized(&self) -> bool {
        self.quantized
    }
}

    /// 将 f64 向量量化为半精度 f32 存储（内存降 50%）。
    pub fn quantize_to_f32(vector: &[(String, f64)]) -> Vec<(String, f32)> {
        vector.iter().map(|(k, v)| (k.clone(), *v as f32)).collect()
    }

    /// 将 f32 半精度反量化为 f64。
    pub fn dequantize_from_f32(vector: &[(String, f32)]) -> Vec<(String, f64)> {
        vector.iter().map(|(k, v)| (k.clone(), *v as f64)).collect()
    }

impl Default for TfIdfIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SemanticIndex for TfIdfIndex {
    fn name(&self) -> &str { &self.name }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<ScoredEpisode>, SemanticError> {
        let docs = self.docs.read().await;
        if docs.is_empty() {
            return Ok(Vec::new());
        }

        let total_docs = docs.len();
        let df = self.df.read().await;

        // 计算查询的 TF-IDF 向量
        let query_tokens = tokenizer::tokenize(query);
        let query_total = query_tokens.len();
        if query_total == 0 {
            return Ok(Vec::new());
        }

        let mut query_tf: HashMap<String, usize> = HashMap::new();
        for token in &query_tokens {
            *query_tf.entry(token.clone()).or_insert(0) += 1;
        }

        let query_vector: Vec<(String, f64)> = query_tf.iter()
            .map(|(term, freq)| {
                let tf_val = tf(*freq, query_total);
                let doc_freq = df.get(term).copied().unwrap_or(1);
                let idf_val = idf(total_docs, doc_freq);
                (term.clone(), tf_val * idf_val)
            })
            .collect();

        if query_vector.is_empty() {
            return Ok(Vec::new());
        }

        // 计算与每个文档的余弦相似度
        let mut scored: Vec<ScoredEpisode> = docs.iter()
            .map(|doc| {
                let doc_vector_f64: Vec<(String, f64)> = doc.vector.iter()
                    .map(|(k, v)| (k.clone(), *v as f64)).collect();
                let score = cosine_similarity(&query_vector, &doc_vector_f64);
                let matched_terms: Vec<String> = query_vector.iter()
                    .filter(|(qt, _)| doc.term_freq.contains_key(qt))
                    .map(|(qt, _)| qt.clone())
                    .collect();

                ScoredEpisode {
                    episode: doc.episode.clone(),
                    score,
                    matched_terms,
                }
            })
            .filter(|s| s.score > 0.0)
            .collect();

        // 按评分降序排列
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // 截取前 limit 条
        scored.truncate(limit);
        Ok(scored)
    }

    async fn index_episode(&self, episode: &Episode) -> Result<(), SemanticError> {
        let mut docs = self.docs.write().await;
        let mut df = self.df.write().await;

        // 增加新文档的 DF
        let text = format!("{} {}", episode.title, episode.summary);
        let unique: std::collections::HashSet<String> = tokenizer::unique_tokens(&text);
        for token in &unique {
            *df.entry(token.clone()).or_insert(0) += 1;
        }

        // 重新计算所有文档的向量（DF 变了）
        let total_docs = docs.len() + 1;
        let new_df = df.clone();

        // 添加新文档
        let tokens = tokenizer::tokenize(&text);
        let total_terms = tokens.len();
        let mut term_freq: HashMap<String, usize> = HashMap::new();
        for token in &tokens {
            *term_freq.entry(token.clone()).or_insert(0) += 1;
        }

        let mut vector: Vec<(String, f64)> = Vec::new();
        for (term, freq) in &term_freq {
            let tf_val = tf(*freq, total_terms);
            let doc_freq = new_df.get(term).copied().unwrap_or(1);
            let idf_val = idf(total_docs, doc_freq);
            vector.push((term.clone(), tf_val * idf_val));
        }
        vector.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let quantized_vector: Vec<(String, f32)> = vector.into_iter()
            .map(|(k, v)| (k, v as f32)).collect();
        docs.push(IndexedDoc {
            episode: episode.clone(),
            vector: quantized_vector,
            term_freq,
        });

        // 重新计算所有旧文档的向量
        let new_total = docs.len();
        let df_clone = &new_df;
        for doc in docs.iter_mut() {
            let text = format!("{} {}", doc.episode.title, doc.episode.summary);
            let tokens = tokenizer::tokenize(&text);
            let total_terms = tokens.len();
            if total_terms == 0 { continue; }

            let mut term_freq_map: HashMap<String, usize> = HashMap::new();
            for token in &tokens {
                *term_freq_map.entry(token.clone()).or_insert(0) += 1;
            }

            let mut new_vector: Vec<(String, f64)> = Vec::new();
            for (term, freq) in &term_freq_map {
                let tf_val = tf(*freq, total_terms);
                let doc_freq = df_clone.get(term).copied().unwrap_or(1);
                let idf_val = idf(new_total, doc_freq);
                new_vector.push((term.clone(), tf_val * idf_val));
            }
            new_vector.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            doc.vector = new_vector.into_iter().map(|(k, v)| (k, v as f32)).collect();
        }

        Ok(())
    }

    async fn index_batch(&self, episodes: &[Episode]) -> Result<(), SemanticError> {
        if episodes.is_empty() {
            return Ok(());
        }

        // 全量重建更高效
        let mut docs = self.docs.write().await;
        let mut df = self.df.write().await;

        // 合并现有和新文档
        let mut all_episodes: Vec<Episode> = docs.iter().map(|d| d.episode.clone()).collect();
        all_episodes.extend_from_slice(episodes);

        // 重建 DF
        *df = Self::build_df(&all_episodes);
        let total_docs = all_episodes.len();

        // 重建向量
        docs.clear();
        for ep in &all_episodes {
            let text = format!("{} {}", ep.title, ep.summary);
            let tokens = tokenizer::tokenize(&text);
            let total_terms = tokens.len();

            let mut term_freq: HashMap<String, usize> = HashMap::new();
            for token in &tokens {
                *term_freq.entry(token.clone()).or_insert(0) += 1;
            }

            let mut vector: Vec<(String, f64)> = Vec::new();
            for (term, freq) in &term_freq {
                let tf_val = tf(*freq, total_terms);
                let doc_freq = df.get(term).copied().unwrap_or(1);
                let idf_val = idf(total_docs, doc_freq);
                vector.push((term.clone(), tf_val * idf_val));
            }
            vector.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let quantized_vector: Vec<(String, f32)> = vector.into_iter()
                .map(|(k, v)| (k, v as f32)).collect();
            docs.push(IndexedDoc {
                episode: ep.clone(),
                vector: quantized_vector,
                term_freq,
            });
        }

        Ok(())
    }

    async fn rebuild_from_store(&self, store: &dyn EpisodeRepository) -> Result<usize, SemanticError> {
        let episodes = store.query(EpisodeQuery::default()).await
            .map_err(|e| SemanticError::StorageError(e.to_string()))?;
        self.index_batch(&episodes).await?;
        Ok(episodes.len())
    }

    fn doc_count(&self) -> usize {
        // 尝试获取锁并返回文档数
        match self.docs.try_read() {
            Ok(docs) => docs.len(),
            Err(_) => 0,
        }
    }

    async fn clear(&self) -> Result<(), SemanticError> {
        let mut docs = self.docs.write().await;
        docs.clear();
        let mut df = self.df.write().await;
        df.clear();
        Ok(())
    }
}

// ─── SemanticError ──────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum SemanticError {
    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("索引错误: {0}")]
    IndexError(String),

    #[error("搜索错误: {0}")]
    SearchError(String),
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use super::*;
    use lingshu_memory_episode::{Episode, EpisodeRepository, EpisodeQuery, InMemoryEpisodeStore};

    fn make_ep(title: &str, summary: &str) -> Episode {
        Episode::new(title, summary, Utc::now())
    }

    #[tokio::test]
    async fn test_empty_index() {
        let index = TfIdfIndex::new();
        let results = index.search("test", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_index_and_search() {
        let index = TfIdfIndex::new();
        index.index_episode(&make_ep("RAG技术", "Retrieval Augmented Generation的原理")).await.unwrap();
        index.index_episode(&make_ep("项目A启动", "项目A正式启动")).await.unwrap();
        index.index_episode(&make_ep("天气", "今天天气很好")).await.unwrap();

        let results = index.search("RAG", 10).await.unwrap();
        assert!(!results.is_empty(), "should find RAG episode");
        assert!(results[0].score > 0.0, "should have positive score");
        assert_eq!(results[0].episode.title, "RAG技术", "RAG should be top result");
    }

    #[tokio::test]
    async fn test_search_ranking() {
        let index = TfIdfIndex::new();
        index.index_episode(&make_ep("项目A讨论", "讨论了项目A的技术方案")).await.unwrap();
        index.index_episode(&make_ep("项目A暂停", "项目A因为供应商问题暂停")).await.unwrap();
        index.index_episode(&make_ep("天气情况", "今天的天气很好")).await.unwrap();

        let results = index.search("项目A", 10).await.unwrap();
        assert_eq!(results.len(), 2, "should find 2 project-A related episodes");
        assert!(results[0].score >= results[1].score, "should be sorted by score");
    }

    #[tokio::test]
    async fn test_index_batch() {
        let index = TfIdfIndex::new();
        let episodes = vec![
            make_ep("事件1", "描述1"),
            make_ep("事件2", "描述2"),
            make_ep("事件3", "描述3"),
        ];
        index.index_batch(&episodes).await.unwrap();

        let results = index.search("事件1", 10).await.unwrap();
        assert_eq!(results.len(), 3, "all episodes share bigram 事件 so they should all match");
    }

    #[tokio::test]
    async fn test_rebuild_from_store() {
        let store = InMemoryEpisodeStore::new();
        store.store(make_ep("测试A", "语义测试A")).await.unwrap();
        store.store(make_ep("测试B", "语义测试B")).await.unwrap();

        let index = TfIdfIndex::new();
        let count = index.rebuild_from_store(&store).await.unwrap();
        assert_eq!(count, 2);

        let results = index.search("语义", 10).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_clear() {
        let index = TfIdfIndex::new();
        index.index_episode(&make_ep("测试", "测试内容")).await.unwrap();
        index.clear().await.unwrap();
        let results = index.search("测试", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_matched_terms() {
        let index = TfIdfIndex::new();
        index.index_episode(&make_ep("RAG技术", "RAG的原理和应用")).await.unwrap();

        let results = index.search("RAG", 10).await.unwrap();
        assert!(!results.is_empty());
        assert!(!results[0].matched_terms.is_empty(), "should have matched terms");
    }

    #[tokio::test]
    async fn test_chinese_search() {
        let index = TfIdfIndex::new();
        index.index_episode(&make_ep("项目A", "项目A的启动和暂停")).await.unwrap();
        index.index_episode(&make_ep("项目B", "项目B的完成")).await.unwrap();

        let results = index.search("暂停", 10).await.unwrap();
        assert_eq!(results.len(), 1, "only project A has '暂停'");
        assert_eq!(results[0].episode.title, "项目A");
    }

    #[tokio::test]
    async fn test_no_match() {
        let index = TfIdfIndex::new();
        index.index_episode(&make_ep("项目A", "项目A")).await.unwrap();
        let results = index.search("xyz_nonexistent_123", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_limit() {
        let index = TfIdfIndex::new();
        for i in 0..10 {
            index.index_episode(&make_ep(&format!("事件{}", i), "共同关键词")).await.unwrap();
        }
        let results = index.search("共同关键词", 3).await.unwrap();
        assert_eq!(results.len(), 3, "should respect limit");
    }

    #[test]
    fn test_tf_idf_functions() {
        assert!((idf(10, 2) - 2.609).abs() < 0.01);
        assert!((idf(10, 0) - 0.0).abs() < 0.01);
        assert!((tf(3, 10) - 0.3).abs() < 0.01);
        assert_eq!(tf(0, 10), 0.0);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![("a".to_string(), 1.0), ("b".to_string(), 2.0)];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![("a".to_string(), 1.0)];
        let b = vec![("b".to_string(), 1.0)];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[("a".to_string(), 1.0)]), 0.0);
    }
}
