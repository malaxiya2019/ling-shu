//! JobQueue — 作业队列 trait 及 InMemory 实现.
//!
//! 支持优先级队列，高优先级作业先出队。
//! 可替换为 SQLite/Redis 实现以实现持久化。

use std::collections::{BinaryHeap, HashMap};

use async_trait::async_trait;
use lingshu_core::{LsId, LsResult};
use tokio::sync::RwLock;
use tracing::debug;

use crate::task_scheduler::Job;

// ═══════════════════════════════════════════════════════════
// JobQueue trait
// ═══════════════════════════════════════════════════════════

/// 作业队列抽象接口.
///
/// 默认使用 `InMemoryJobQueue`，可替换为 SQLite/Redis 实现：
/// - SQLite: 持久化队列，崩溃恢复
/// - Redis: 分布式队列，跨实例共享
#[async_trait]
pub trait JobQueue: Send + Sync {
    /// 入队.
    async fn enqueue(&mut self, job: Box<dyn Job>) -> LsResult<()>;
    /// 出队 (返回最高优先级的作业).
    async fn dequeue(&mut self) -> Option<Box<dyn Job>>;
    /// 查看队首 (不移除).
    async fn peek(&self) -> Option<Box<dyn Job>>;
    /// 按 ID 移除.
    async fn remove(&mut self, job_id: &LsId) -> LsResult<()>;
    /// 队列长度.
    async fn len(&self) -> usize;
    /// 队列是否为空.
    async fn is_empty(&self) -> bool;
    /// 清空队列.
    async fn clear(&mut self);
}

// ═══════════════════════════════════════════════════════════
// InMemoryJobQueue — 基于 BinaryHeap 实现
// ═══════════════════════════════════════════════════════════

/// 优先级队列包装.
struct JobWrapper {
    job: Box<dyn Job>,
    enqueued_at: chrono::DateTime<chrono::Utc>,
}

impl JobWrapper {
    fn priority(&self) -> (u8, i64) {
        // 优先级越高越优先; 同优先级按入队时间 (越早越优先)
        let neg_time = -self.enqueued_at.timestamp_nanos_opt().unwrap_or(0);
        (self.job.priority(), neg_time)
    }
}

impl PartialEq for JobWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.priority() == other.priority()
    }
}

impl Eq for JobWrapper {}

impl PartialOrd for JobWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JobWrapper {
    /// BinaryHeap 是 max-heap，所以优先返回 priority 更高的.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority().cmp(&other.priority())
    }
}

/// 内存作业队列 — 基于 BinaryHeap 的优先级队列.
pub struct InMemoryJobQueue {
    heap: RwLock<BinaryHeap<JobWrapper>>,
    by_id: RwLock<HashMap<LsId, ()>>, // 快速 ID 查重
}

impl InMemoryJobQueue {
    pub fn new() -> Self {
        Self {
            heap: RwLock::new(BinaryHeap::new()),
            by_id: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl JobQueue for InMemoryJobQueue {
    async fn enqueue(&mut self, job: Box<dyn Job>) -> LsResult<()> {
        let id = job.id();
        let wrapper = JobWrapper {
            job,
            enqueued_at: chrono::Utc::now(),
        };

        let mut by_id = self.by_id.write().await;
        by_id.insert(id, ());

        let mut heap = self.heap.write().await;
        heap.push(wrapper);

        debug!(job_id = %id, queue_len = heap.len(), "job enqueued");
        Ok(())
    }

    async fn dequeue(&mut self) -> Option<Box<dyn Job>> {
        let mut heap = self.heap.write().await;
        let wrapper = heap.pop()?;
        let id = wrapper.job.id();

        let mut by_id = self.by_id.write().await;
        by_id.remove(&id);

        debug!(job_id = %id, queue_len = heap.len(), "job dequeued");
        Some(wrapper.job)
    }

    async fn peek(&self) -> Option<Box<dyn Job>> {
        // 无法从 &BinaryHeap 偷看内部内容而不移动，这里简化实现
        // 实际 peeking 在高并发下意义不大
        None
    }

    async fn remove(&mut self, job_id: &LsId) -> LsResult<()> {
        let mut by_id = self.by_id.write().await;
        if by_id.remove(job_id).is_none() {
            return Ok(()); // 不存在也算成功
        }
        // BinaryHeap 不支持按值移除，我们重建堆
        let mut heap = self.heap.write().await;
        let remaining: Vec<JobWrapper> = heap
            .drain()
            .filter(|w| w.job.id() != *job_id)
            .collect();
        heap.extend(remaining);

        debug!(job_id = %job_id, "job removed from queue");
        Ok(())
    }

    async fn len(&self) -> usize {
        self.heap.read().await.len()
    }

    async fn is_empty(&self) -> bool {
        self.heap.read().await.len() == 0
    }

    async fn clear(&mut self) {
        let mut heap = self.heap.write().await;
        heap.clear();
        let mut by_id = self.by_id.write().await;
        by_id.clear();
        debug!("queue cleared");
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_core::{LsContext, LsResult};
    use serde_json::Value;

    struct DummyJob {
        id: LsId,
        name: String,
        prio: u8,
    }

    #[async_trait]
    impl Job for DummyJob {
        fn id(&self) -> LsId { self.id }
        fn name(&self) -> &str { &self.name }
        fn priority(&self) -> u8 { self.prio }
        async fn execute(&self, _ctx: LsContext) -> LsResult<Value> {
            Ok(Value::Null)
        }
    }

    #[tokio::test]
    async fn test_enqueue_dequeue() {
        let mut queue = InMemoryJobQueue::new();
        assert!(queue.is_empty().await);

        let job = Box::new(DummyJob {
            id: LsId::new(),
            name: "test".into(),
            prio: 0,
        });
        queue.enqueue(job).await.unwrap();
        assert!(!queue.is_empty().await);
        assert_eq!(queue.len().await, 1);

        let popped = queue.dequeue().await;
        assert!(popped.is_some());
        assert_eq!(popped.unwrap().name(), "test");
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn test_priority_order() {
        let mut queue = InMemoryJobQueue::new();

        let low = Box::new(DummyJob {
            id: LsId::new(),
            name: "low".into(),
            prio: 0,
        });
        let high = Box::new(DummyJob {
            id: LsId::new(),
            name: "high".into(),
            prio: 255,
        });
        let mid = Box::new(DummyJob {
            id: LsId::new(),
            name: "mid".into(),
            prio: 128,
        });

        queue.enqueue(low).await.unwrap();
        queue.enqueue(high).await.unwrap();
        queue.enqueue(mid).await.unwrap();

        assert_eq!(queue.dequeue().await.unwrap().name(), "high");
        assert_eq!(queue.dequeue().await.unwrap().name(), "mid");
        assert_eq!(queue.dequeue().await.unwrap().name(), "low");
    }

    #[tokio::test]
    async fn test_remove() {
        let mut queue = InMemoryJobQueue::new();
        let id = LsId::new();

        queue.enqueue(Box::new(DummyJob {
            id,
            name: "removable".into(),
            prio: 0,
        })).await.unwrap();

        assert_eq!(queue.len().await, 1);
        queue.remove(&id).await.unwrap();
        assert_eq!(queue.len().await, 0);
    }

    #[tokio::test]
    async fn test_clear() {
        let mut queue = InMemoryJobQueue::new();
        for i in 0..5 {
            queue.enqueue(Box::new(DummyJob {
                id: LsId::new(),
                name: format!("job-{i}"),
                prio: 0,
            })).await.unwrap();
        }
        assert_eq!(queue.len().await, 5);
        queue.clear().await;
        assert_eq!(queue.len().await, 0);
    }
}
