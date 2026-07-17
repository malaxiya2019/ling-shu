//! LSMemoryConsolidation — 记忆巩固引擎。
//!
//! 将短期、零散的 Episode 提炼为结构化长期记忆。
//! 模拟人脑中"海马体回放 → 皮层萃取规律"的过程。
//!
//! # 架构
//!
//! ```text
//! 短期 Episode（零散事件流）
//!       │
//!       ▼
//!  EpisodeAnalyzer（按实体/时间/主题分组）
//!       │
//!       ▼
//!  ConsolidationStrategy（策略层：摘要/去重/画像）
//!       │
//!       ▼
//!  ConsolidatedMemory（巩固后的长期记忆）
//!       │
//!       ▼
//!  Episode Store / EvidenceGraph（持久化）
//! ```
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use lingshu_memory_consolidation::{
//!     ConsolidationEngine, ConsolidationConfig,
//!     strategies::{SummarizeStrategy, DedupStrategy},
//! };
//!
//! let engine = ConsolidationEngine::new(store, ConsolidationConfig::default());
//! engine.add_strategy(Box::new(SummarizeStrategy::new()));
//! engine.add_strategy(Box::new(DedupStrategy::new()));
//!
//! let report = engine.run_consolidation().await?;
//! println!("巩固完成: {} 条记忆已合并", report.consolidated_count);
//! ```

mod types;
pub use types::*;

mod strategy;
pub use strategy::*;

mod analyzer;
pub use analyzer::*;

mod engine;
pub use engine::*;

mod importance;
pub use importance::*;

mod store;
pub use store::*;

mod forgetting;
pub use forgetting::*;

mod workflow;
pub use workflow::*;
