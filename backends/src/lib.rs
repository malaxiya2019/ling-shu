//! LSBackends — 具体后端实现.
//!
//! ## Feature Flags
//! - `openai` — OpenAI LLM + Embedding (默认启用)
//! - `anthropic` — Anthropic Messages API
//! - `groq` — Groq API (OpenAI 兼容格式)
//! - `mock` — 零依赖模拟引擎 (默认启用)
//! - `vector-store-sqlite` — SQLite 持久化向量存储
//! - `vector-store-pg` — PostgreSQL 持久化向量存储
//!
//! ## LLM 提供商
//! - `openai` — OpenAI Chat Completions (GPT-4, GPT-4o, etc.)
//! - `anthropic` — Anthropic Messages API (Claude 3, Claude 3.5)
//! - `groq` — Groq API (Llama, Mixtral) — 复用 OpenAI 兼容格式
//! - `mock` — 零依赖模拟引擎 (开发/测试)
//!
//! ## Vector Store
//! - `in_memory` — 基于余弦相似度的纯内存实现
//! - `sqlite` — 基于 SQLite 的持久化向量存储 (feature: `vector-store-sqlite`)
//! - `pg` — 基于 PostgreSQL 的持久化向量存储 (feature: `vector-store-pg`)

pub mod agent_default;
#[cfg(feature = "vector-store-sqlite")]
pub mod memory_sqlite;
#[cfg(feature = "openai")]
pub mod tools;
pub mod workflow;

pub mod embedding_openai;
pub mod knowledge_mem;
#[cfg(feature = "anthropic")]
pub mod llm_anthropic;
pub mod llm_factory;
#[cfg(any(feature = "openai", feature = "groq"))]
pub mod llm_openai;
pub mod llm_retry;
#[cfg(feature = "mock")]
pub mod mock_llm;
pub mod vector_memory;
pub mod vector_store_mem;
#[cfg(feature = "vector-store-pg")]
pub mod vector_store_pg;
#[cfg(feature = "mock")]
#[cfg(feature = "vector-store-sqlite")]
pub mod vector_store_sqlite;

pub use agent_default::{AgentConfig, DefaultAgent};
#[cfg(feature = "openai")]
pub use embedding_openai::OpenAiEmbedding;
pub use knowledge_mem::InMemoryKnowledge;
#[cfg(feature = "anthropic")]
pub use llm_anthropic::AnthropicLlm;
pub use llm_factory::build_llm;
#[cfg(any(feature = "openai", feature = "groq"))]
pub use llm_openai::OpenAiLlm;
pub use llm_retry::{with_retry, RetryLlm};
#[cfg(feature = "vector-store-sqlite")]
pub use memory_sqlite::MemorySQLite;
#[cfg(feature = "mock")]
pub use mock_llm::MockLlm;
pub use vector_memory::VectorMemory;
pub use vector_store_mem::InMemoryVectorStore;
#[cfg(feature = "vector-store-pg")]
pub use vector_store_pg::PgVector;
#[cfg(feature = "vector-store-sqlite")]
pub use vector_store_sqlite::SQLiteVector;
