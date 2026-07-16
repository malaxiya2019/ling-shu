//! LSMemorySemantic — 语义记忆层。
//!
//! 提供无需外部 Embedding API 的本地语义搜索能力。
//! 基于 TF-IDF + 字符 n-gram，适合中文和英文混合文本。
//!
//! # 架构
//!
//! ```text
//! Episode Store
//!      │
//!      ▼
//! SemanticIndex (trait)
//!      │
//!  ┌───┴───┐
//!  │       │
//! TfIdf  (未来: OpenAI/Qdrant Embedding)
//! Index
//!      │
//!      ▼
//! SemanticWorkflow (MemoryWorkflow impl)
//!      │
//!      ▼
//! EvidenceGraph
//! ```
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use lingshu_memory_semantic::{SemanticIndex, TfIdfIndex};
//!
//! let index = TfIdfIndex::new();
//! index.index_episode(&episode).await?;
//! let results = index.search("什么是RAG", 5).await?;
//! ```

mod index;
mod tokenizer;
mod workflow;

pub use index::*;
pub use tokenizer::*;
pub use workflow::*;
