//! LSTraits — Lingshu 公共接口契约 (v5.0).
//!
//! 定义全系统模块间的 **唯一交互标准** — 所有跨 crate 调用必须通过此处的 trait。
//!
//! # Trait 一览
//!
//! | Trait | 模块 | 用途 |
//! |-------|------|------|
//! | [`LlmProvider`] | `llm` | LLM 调用（兼容 OpenAI / Anthropic / Ollama） |
//! | [`VectorStore`] | `vector_store` | 向量存储 CRUD + 语义搜索 |
//! | [`MemoryProvider`] | `memory` | Agent 记忆存储与检索 |
//! | [`ToolProvider`] | `tool` | 工具定义与执行 |
//! | [`EmbeddingProvider`] | `embedding` | 文本嵌入生成 |
//! | [`EventBus`] | `event_bus` | 事件发布/订阅 |
//! | [`StorageProvider`] | `storage` | 键值/文件存储 |
//! | [`AgentDriver`] | `agent` | Agent 驱动（执行/停止/状态） |
//! | [`DatabaseProvider`] | `database` | 数据库访问抽象 |
//! | [`PluginProvider`] | `plugin` | 插件加载与生命周期 |
//! | [`SchedulerProvider`] | `scheduler` | 任务调度接口 |
//! | [`RuntimeProvider`] | `runtime` | 运行时接口 |
//! | [`Repository`] | `repository` | 数据仓库模式 |
//! | [`KnowledgeProvider`] | `knowledge` | 知识图谱接口 |
//! | [`VoiceProvider`] | `voice` | 语音合成/识别 |
//!
//! # 一致性约定
//!
//! - 所有 trait 均标注 `#[async_trait] + Send + Sync + 'static`
//! - 所有方法统一返回 `LsResult<T>` (来自 `lingshu_core`)
//! - 所有异步方法接受 `&self`（实现内部通过 `Arc` 共享状态）
//! - 扩展新 trait 需在 `pub mod` + `pub use` 两处注册

pub mod agent;
pub mod database;
pub mod embedding;
pub mod event_bus;
pub mod knowledge;
pub mod llm;
pub mod memory;
pub mod plugin;
pub mod repository;
pub mod runtime;
pub mod scheduler;
pub mod storage;
pub mod tool;
pub mod vector_store;
pub mod voice;

/// Agent 驱动接口 — 创建、执行、停止、查询 Agent。
///
/// 包含: `AgentDriver`, `AgentStatus`, `AgentOutput`, `AgentCapability`
pub use agent::*;

/// 数据库访问抽象 — 连接、查询、事务。
///
/// 包含: `DatabaseProvider`, `DatabaseConfig`, `QueryResult`
pub use database::*;

/// 嵌入生成接口 — 将文本转换为向量。
///
/// 包含: `EmbeddingProvider`, `EmbeddingConfig`
pub use embedding::*;

/// 事件总线接口 — 发布/订阅 / 过滤 / 通配符匹配。
///
/// 包含: `EventBus`, `Event`, `Subscription`
pub use event_bus::*;

/// 知识图谱接口 — 实体 / 关系 / 图查询。
///
/// 包含: `KnowledgeProvider`, `Entity`, `Relation`
pub use knowledge::*;

/// LLM 提供者接口 — 兼容 OpenAI / Anthropic / Ollama / 自定义。
///
/// 包含: `LlmProvider`, `LlmRequest`, `LlmResponse`, `LlmMessage`, `LlmRole`, `ToolCall`
pub use llm::*;

/// 记忆存储接口 — Agent 对话记忆 / 摘要 / 检索。
///
/// 包含: `MemoryProvider`, `MemoryEntry`, `MemoryQuery`, `MemorySummary`
pub use memory::*;

/// 插件接口 — 加载、初始化、执行、卸载插件。
///
/// 包含: `PluginProvider`, `PluginManifest`, `PluginContext`
pub use plugin::*;

/// 数据仓库接口 — CRUD 抽象。
///
/// 包含: `Repository`, `RepositoryError`
pub use repository::*;

/// 运行时接口 — 启动 / 停止 / 健康检查 / 配置。
///
/// 包含: `RuntimeProvider`, `RuntimeConfig`
pub use runtime::*;

/// 调度器接口 — 提交 / 取消 / 查询任务。
///
/// 包含: `SchedulerProvider`, `Task`, `TaskStatus`, `ScheduleResult`
pub use scheduler::*;

/// 存储接口 — 键值 / 文件 / 对象存储。
///
/// 包含: `StorageProvider`, `StorageConfig`
pub use storage::*;

/// 工具接口 — 注册 / 调用 / 发现工具。
///
/// 包含: `ToolProvider`, `ToolDef`, `ToolCall`, `ToolResult`
pub use tool::*;

/// 向量存储接口 — 集合管理 / Upsert / 语义搜索。
///
/// 包含: `VectorStore`, `VectorRecord`, `SearchResult`, `CollectionInfo`
pub use vector_store::*;

/// 语音接口 — 语音合成 / 语音识别。
///
/// 包含: `VoiceProvider`, `VoiceRequest`, `VoiceResponse`
pub use voice::*;
