//! Memory types — shared data structures for the memory system

use serde::{Deserialize, Serialize};

/// A single memory item stored in the buffer or long-term storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: serde_json::Value,
}

impl MemoryItem {
    pub fn new(session_id: &str, role: &str, content: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            metadata: serde_json::json!({}),
        }
    }

    /// Attach metadata to this memory item
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        if let Some(map) = self.metadata.as_object_mut() {
            map.insert(key.to_string(), value);
        }
        self
    }
}

/// Query for retrieving memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub session_id: Option<String>,
    pub query: Option<String>,
    pub limit: usize,
    pub offset: usize,
    pub min_relevance: Option<f64>,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            session_id: None,
            query: None,
            limit: 50,
            offset: 0,
            min_relevance: None,
        }
    }
}

/// Result of a memory retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    pub items: Vec<MemoryItem>,
    pub total: usize,
    pub query_time_ms: u64,
}

/// Configuration for the memory system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Max items in short-term buffer
    pub buffer_capacity: usize,
    /// Whether to auto-sync buffer to long-term storage
    pub auto_sync: bool,
    /// Sync interval in seconds
    pub sync_interval_secs: u64,
    /// LLM model for summarization (if enabled)
    pub summary_model: Option<String>,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            buffer_capacity: 100,
            auto_sync: true,
            sync_interval_secs: 60,
            summary_model: None,
        }
    }
}
