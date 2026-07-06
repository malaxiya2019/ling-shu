//! LSMemory — Lingshu dual-storage memory system.
//!
//! Provides short-term buffer memory (ChatBuffer) and long-term vector storage
//! (VectorMemory trait + InMemoryVectorStore for development).
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │           Memory trait                   │
//! │  store() · recall() · search()          │
//! └──────────┬──────────────────────────────┘
//!            │
//!      ┌─────┴──────┐
//!      │            │
//!   ChatBuffer   VectorMemory
//!   (short-term)  (long-term)
//! ──────────────────────────────────────────────
//! ```

pub mod buffer;
pub mod memory;
pub mod types;
pub mod vector;
pub mod session;
pub mod graph;

pub use buffer::ChatBuffer;
pub use memory::{DefaultMemory, Memory};
pub use types::{MemoryConfig, MemoryItem, MemoryQuery, MemoryResult};
pub use session::SessionMemoryManager;
pub use graph::*;
pub use vector::{InMemoryVectorStore, VectorMemory, VectorRecord};
