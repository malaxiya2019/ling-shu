//! ChatBuffer — Short-term sliding window memory buffer
//!
//! Stores recent conversation history as a fixed-size ring buffer.
//! Oldest items are evicted when capacity is exceeded.

use crate::types::MemoryItem;
use std::collections::VecDeque;
use tokio::sync::RwLock;
use tracing::debug;

/// Ring buffer for short-term conversation memory
pub struct ChatBuffer {
    items: RwLock<VecDeque<MemoryItem>>,
    capacity: usize,
    session_id: String,
}

impl ChatBuffer {
    /// Create a new buffer with given capacity
    pub fn new(session_id: &str, capacity: usize) -> Self {
        Self {
            items: RwLock::new(VecDeque::with_capacity(capacity)),
            capacity,
            session_id: session_id.to_string(),
        }
    }

    /// Add a memory item (evicts oldest if at capacity)
    pub async fn add(&self, item: MemoryItem) {
        let mut items = self.items.write().await;
        if items.len() >= self.capacity {
            items.pop_front();
        }
        items.push_back(item);
        debug!("Buffer item added (size: {})", items.len());
    }

    /// Add a simple chat message
    pub async fn add_message(&self, role: &str, content: &str) {
        let item = MemoryItem::new(&self.session_id, role, content);
        self.add(item).await;
    }

    /// Get recent N items
    pub async fn recent(&self, n: usize) -> Vec<MemoryItem> {
        let items = self.items.read().await;
        items.iter().rev().take(n).cloned().rev().collect()
    }

    /// Get all items
    pub async fn all(&self) -> Vec<MemoryItem> {
        self.items.read().await.iter().cloned().collect()
    }

    /// Get items since a given timestamp
    pub async fn since(&self, since: chrono::DateTime<chrono::Utc>) -> Vec<MemoryItem> {
        let items = self.items.read().await;
        items
            .iter()
            .filter(|item| item.timestamp > since)
            .cloned()
            .collect()
    }

    /// Clear the buffer
    pub async fn clear(&self) {
        self.items.write().await.clear();
        debug!("Buffer cleared");
    }

    /// Current buffer size
    pub async fn len(&self) -> usize {
        self.items.read().await.len()
    }

    /// Check if buffer is empty
    pub async fn is_empty(&self) -> bool {
        self.items.read().await.is_empty()
    }

    /// Get buffer capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the conversation context as a formatted string
    pub async fn context_string(&self) -> String {
        let items = self.items.read().await;
        items
            .iter()
            .map(|item| format!("{}: {}", item.role, item.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format as LLM messages (for chat completion API)
    pub async fn to_llm_messages(&self) -> Vec<lingshu_traits::llm::LlmMessage> {
        let items = self.items.read().await;
        items
            .iter()
            .map(|item| {
                let role = if item.role == "assistant" {
                    lingshu_traits::llm::LlmRole::Assistant
                } else {
                    lingshu_traits::llm::LlmRole::User
                };
                lingshu_traits::llm::LlmMessage {
                    role,
                    content: item.content.clone(),
                    name: None,
                    tool_calls: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_and_recent() {
        let buf = ChatBuffer::new("test-session", 10);
        buf.add_message("user", "Hello").await;
        buf.add_message("assistant", "Hi there").await;

        assert_eq!(buf.len().await, 2);
        let recent = buf.recent(1).await;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].content, "Hi there");
    }

    #[tokio::test]
    async fn test_eviction() {
        let buf = ChatBuffer::new("test-session", 3);
        buf.add_message("user", "msg1").await;
        buf.add_message("user", "msg2").await;
        buf.add_message("user", "msg3").await;
        buf.add_message("user", "msg4").await;

        assert_eq!(buf.len().await, 3);
        let all = buf.all().await;
        assert_eq!(all[0].content, "msg2");
        assert_eq!(all[2].content, "msg4");
    }

    #[tokio::test]
    async fn test_clear() {
        let buf = ChatBuffer::new("test-session", 10);
        buf.add_message("user", "Hello").await;
        buf.clear().await;
        assert!(buf.is_empty().await);
    }

    #[tokio::test]
    async fn test_context_string() {
        let buf = ChatBuffer::new("test-session", 10);
        buf.add_message("user", "Hello").await;
        buf.add_message("assistant", "Hi there").await;
        let ctx = buf.context_string().await;
        assert!(ctx.contains("user: Hello"));
        assert!(ctx.contains("assistant: Hi there"));
    }

    #[tokio::test]
    async fn test_since() {
        let buf = ChatBuffer::new("test-session", 10);
        let past = chrono::Utc::now() - chrono::Duration::hours(1);
        buf.add_message("user", "old").await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        buf.add_message("user", "new").await;

        let items = buf.since(past).await;
        assert_eq!(items.len(), 2);
    }
}
