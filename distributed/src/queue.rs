//! Distributed queue — partitioned message queue for inter-node communication

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// A message in the distributed queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    pub id: String,
    pub topic: String,
    pub payload: String,
    pub created_at: i64,
    pub ttl_secs: Option<u64>,
}

/// Queue configuration
#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub max_queue_size: usize,
    pub default_ttl_secs: Option<u64>,
    pub batch_size: usize,
    pub poll_interval: Duration,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 10_000,
            default_ttl_secs: Some(3600),
            batch_size: 100,
            poll_interval: Duration::from_millis(100),
        }
    }
}

/// A single topic partition
struct TopicPartition {
    messages: VecDeque<QueueMessage>,
    unacked: HashMap<String, Instant>,
}

/// Distributed message queue
pub struct DistributedQueue {
    config: QueueConfig,
    partitions: RwLock<HashMap<String, TopicPartition>>,
}

impl DistributedQueue {
    pub fn new(config: QueueConfig) -> Self {
        Self {
            config,
            partitions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn publish(&self, topic: &str, payload: &str) -> QueueMessage {
        let mut parts = self.partitions.write().await;
        let partition = parts.entry(topic.to_string()).or_insert_with(|| {
            TopicPartition {
                messages: VecDeque::new(),
                unacked: HashMap::new(),
            }
        });

        let msg = QueueMessage {
            id: uuid::Uuid::new_v4().to_string(),
            topic: topic.to_string(),
            payload: payload.to_string(),
            created_at: chrono::Utc::now().timestamp(),
            ttl_secs: self.config.default_ttl_secs,
        };

        if partition.messages.len() >= self.config.max_queue_size {
            partition.messages.pop_front();
            warn!("Queue full for topic {}, evicted oldest message", topic);
        }

        partition.messages.push_back(msg.clone());
        debug!("Published message {} to topic {}", msg.id, topic);
        msg
    }

    pub async fn consume(
        &self,
        topic: &str,
        batch_size: Option<usize>,
    ) -> Vec<QueueMessage> {
        let batch = batch_size.unwrap_or(self.config.batch_size);
        let mut parts = self.partitions.write().await;
        let partition = match parts.get_mut(topic) {
            Some(p) => p,
            None => return vec![],
        };

        let mut batch_msgs = Vec::new();
        let now = Instant::now();

        while batch_msgs.len() < batch {
            let msg = match partition.messages.pop_front() {
                Some(m) => m,
                None => break,
            };

            if let Some(ttl) = msg.ttl_secs {
                let age = chrono::Utc::now().timestamp() - msg.created_at;
                if age > ttl as i64 {
                    debug!("Message {} expired, skipping", msg.id);
                    continue;
                }
            }

            partition.unacked.insert(msg.id.clone(), now);
            batch_msgs.push(msg);
        }

        debug!("Consumed {} messages from topic {}", batch_msgs.len(), topic);
        batch_msgs
    }

    pub async fn ack(&self, topic: &str, msg_id: &str) -> bool {
        let mut parts = self.partitions.write().await;
        match parts.get_mut(topic) {
            Some(partition) => partition.unacked.remove(msg_id).is_some(),
            None => false,
        }
    }

    pub async fn depth(&self, topic: &str) -> usize {
        let parts = self.partitions.read().await;
        parts.get(topic).map_or(0, |p| p.messages.len())
    }

    pub async fn topics(&self) -> Vec<String> {
        let parts = self.partitions.read().await;
        parts.keys().cloned().collect()
    }

    pub async fn start_reaper(&self) {
        info!("Queue reaper started (interval: {:?})", self.config.poll_interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_publish_consume() {
        let queue = DistributedQueue::new(QueueConfig::default());
        queue.publish("test", "hello").await;
        queue.publish("test", "world").await;
        assert_eq!(queue.depth("test").await, 2);

        let msgs = queue.consume("test", Some(10)).await;
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].payload, "hello");
        assert_eq!(msgs[1].payload, "world");
    }

    #[tokio::test]
    async fn test_ack() {
        let queue = DistributedQueue::new(QueueConfig::default());
        let msg = queue.publish("test", "data").await;
        let consumed = queue.consume("test", Some(1)).await;
        assert_eq!(consumed.len(), 1);
        assert!(queue.ack("test", &msg.id).await);
        assert!(!queue.ack("test", &msg.id).await);
    }

    #[tokio::test]
    async fn test_topics() {
        let queue = DistributedQueue::new(QueueConfig::default());
        queue.publish("topic-a", "a").await;
        queue.publish("topic-b", "b").await;
        let topics = queue.topics().await;
        assert_eq!(topics.len(), 2);
        assert!(topics.contains(&"topic-a".to_string()));
        assert!(topics.contains(&"topic-b".to_string()));
    }

    #[tokio::test]
    async fn test_empty_consume() {
        let queue = DistributedQueue::new(QueueConfig::default());
        let msgs = queue.consume("nonexistent", Some(10)).await;
        assert!(msgs.is_empty());
    }
}
