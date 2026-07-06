//! LSCodeAnalyzer — Lingshu 代码分析引擎.
//!
//! 参照 Understand Anything 的代码分析管道，提供：
//! - 文件扫描与分类（代码/配置/文档/基础设施/脚本）
//! - 结构提取（函数/类/导入的正则快速提取）
//! - LLM 语义分析接口（摘要、标签、复杂度评估）
//! - 知识图谱自动生成
//!
//! ## 架构
//!
//! ```text
//! ┌────────────────────────────────────────────┐
//! │            CodeAnalyzer                     │
//! │  ┌──────────┐ ┌──────────┐ ┌────────────┐ │
//! │  │ Scanner  │ │ Extractor│ │ Semantic   │ │
//! │  │(文件扫描) │ │(结构提取) │ │ Analyzer   │ │
//! │  └──────────┘ └──────────┘ │ (LLM 语义) │ │
//! │  ┌──────────────────────┐  └────────────┘ │
//! │  │   GraphGenerator    │                  │
//! │  │ (扫描→知识图谱)     │                  │
//! │  └──────────────────────┘                  │
//! └────────────────────────────────────────────┘
//! ```
//!
//! ## LLM 语义分析
//!
//! `llm_analyzer` 模块提供 LLM 驱动的语义分析能力：
//! - `LlmAnalyzer` — 基于 `dyn Llm` 的语义分析器，生成摘要、标签、复杂度
//! - `EnrichmentQueue` — 异步 enrichment 队列，支持后台批量处理
//! - 支持 Hybrid 模式：规则分析首轮 + LLM 异步 enrichment

pub mod analyzer;
pub mod extractor;
pub mod generator;
pub mod llm_analyzer;
pub mod observer;
pub mod scanner;
pub mod tool;

pub use analyzer::{AnalysisResult, ProjectSummary, SemanticAnalyzer};
pub use extractor::{ExtractedClass, ExtractedFunction, ExtractionResult, StructureExtractor};
pub use generator::GraphGenerator;
pub use llm_analyzer::{
    EnrichmentCallback, EnrichmentQueue, EnrichmentTask, LlmAnalyzer, LlmAnalyzerConfig,
};
pub use observer::{
    ChangeCollector, FileChangeCallback, FileChangeEvent, FileChangeKind, FileObserver,
    NotifyFileObserver, PollingFileObserver,
};
pub use scanner::{FileCategory, FileEntry, FileScanner};
pub use tool::CodeAnalysisTool;
