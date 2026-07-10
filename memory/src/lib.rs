//! LSMemory — Lingshu dual-storage memory system.
//!
//! Provides short-term buffer memory (ChatBuffer), long-term vector storage
//! (VectorMemory), memory summarization (MemorySummarizer), and memory
//! consolidation (MemoryConsolidator).
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────┐
//! │               SessionMemoryManager                     │
//! │  (per-session Memory instance management)              │
//! └──────────┬─────────────────────────────────────────────┘
//!            │
//!       ┌────┴───────────────────┐
//!       │    DefaultMemory        │
//!       │  ┌───────────────────┐  │
//!       │  │   ChatBuffer      │  │  short-term (ring buffer)
//!       │  │   (short-term)    │  │
//!       │  └────────┬──────────┘  │
//!       │           │             │
//!       │  ┌────────▼──────────┐  │
//!       │  │ MemorySummarizer  │  │  LLM-based summarization
//!       │  └────────┬──────────┘  │
//!       │           │             │
//!       │  ┌────────▼──────────┐  │
//!       │  │MemoryConsolidator │  │  auto-sync to long-term
//!       │  └────────┬──────────┘  │
//!       └───────────┼─────────────┘
//!                   │
//!          ┌────────▼────────┐
//!          │  VectorMemory   │  long-term vector storage
//!          │  (InMemory /    │  (Qdrant / SQLite)
//!          │   Qdrant/ SQLite)│
//!          └─────────────────┘
//! ──────────────────────────────────────────────────────────
//! ```

pub mod buffer;
pub mod consolidation;
pub mod graph;
pub mod memory;
pub mod session;
pub mod summarization;
pub mod types;
pub mod vector;

pub use buffer::ChatBuffer;
pub use consolidation::{ConsolidationPolicy, ConsolidationResult, ConsolidationTrigger, Importance, LongTermStore, MemoryConsolidator};
pub use graph::*;
pub use memory::{DefaultMemory, Memory};
pub use session::SessionMemoryManager;
pub use summarization::{MemorySummarizer, MemorySummary, SummarizationConfig, SummarizationStrategy, SummarizerLlm};
pub use types::{MemoryConfig, MemoryItem, MemoryQuery, MemoryResult};
pub use vector::{InMemoryVectorStore, VectorMemory, VectorRecord};
