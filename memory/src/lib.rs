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
pub mod graph;
pub mod memory;
pub mod session;
pub mod types;
pub mod vector;

pub use buffer::ChatBuffer;
pub use graph::*;
pub use memory::{DefaultMemory, Memory};
pub use session::SessionMemoryManager;
pub use types::{MemoryConfig, MemoryItem, MemoryQuery, MemoryResult};
pub use vector::{InMemoryVectorStore, VectorMemory, VectorRecord};
