//! MemoryConsolidation — 记忆合并与持久化
//!
//! 将短期缓冲（ChatBuffer）中的重要/旧条目自动合并到长期存储（SQLite）。
//! 支持两种触发模式：
//! - Size-based: 缓冲条目数超过阈值时
//! - Time-based: 固定时间间隔
//!
//! # 架构
//!
//! ```text
//! ┌──────────────┐     ┌──────────────────┐     ┌───────────┐
//! │  ChatBuffer  │────→│ Consolidation    │────→│ SQLite    │
//! │  (short-term)│     │  Policy + Filter │     │ (long-term)│
//! └──────────────┘     └──────────────────┘     └───────────┘
//!                             │
//!                             ├── age > threshold ──→ persist all
//!                             ├── importance > threshold ──→ persist
//!                             └── buffer full ──→ persist oldest
//! ```

use crate::types::MemoryItem;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use lingshu_core::{LsContext, LsResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// 合并触发策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsolidationTrigger {
    /// 基于缓冲大小
    Size(usize),
    /// 基于时间间隔（秒）
    Interval(u64),
    /// 两者都满足时才触发
    Both(usize, u64),
}

impl Default for ConsolidationTrigger {
    fn default() -> Self {
        Self::Both(100, 300) // 100 条或 5 分钟
    }
}

/// 条目重要性评分
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum Importance {
    /// 低重要性（闲聊等）
    Low = 0,
    /// 中等重要性（一般对话）
    Medium = 1,
    /// 高重要性（决策、关键信息）
    High = 2,
    /// 关键（用户偏好、系统配置等）
    Critical = 3,
}

impl Importance {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "critical" | "关键" => Self::Critical,
            "high" | "高" => Self::High,
            "low" | "低" => Self::Low,
            _ => Self::Medium,
        }
    }
}

/// 合并策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationPolicy {
    /// 触发条件
    pub trigger: ConsolidationTrigger,
    /// 条目保留的最长时间（超过此时间的条目会被合并到长期存储）
    pub max_age_seconds: i64,
    /// 最低重要性阈值（低于此的条目暂时不合并）
    pub min_importance: Importance,
    /// 合并后是否从缓冲中移除
    pub remove_after_consolidation: bool,
    /// 是否启用自动合并
    pub auto_consolidate: bool,
}

impl Default for ConsolidationPolicy {
    fn default() -> Self {
        Self {
            trigger: ConsolidationTrigger::default(),
            max_age_seconds: 1800, // 30 分钟
            min_importance: Importance::Low,
            remove_after_consolidation: true,
            auto_consolidate: true,
        }
    }
}

/// 长期存储接口（抽象 SQLite 或其他后端）
#[async_trait]
pub trait LongTermStore: Send + Sync {
    /// 批量存储记忆条目
    async fn store_batch(&self, ctx: &LsContext, items: Vec<MemoryItem>) -> LsResult<usize>;
    /// 检索长期记忆
    async fn retrieve(&self, ctx: &LsContext, session_id: &str, limit: usize) -> LsResult<Vec<MemoryItem>>;
    /// 搜索长期记忆
    async fn search(&self, ctx: &LsContext, session_id: &str, query: &str, limit: usize) -> LsResult<Vec<MemoryItem>>;
    /// 清除会话的长期记忆
    async fn clear_session(&self, ctx: &LsContext, session_id: &str) -> LsResult<usize>;
}

// ── 合并管理器 ──────────────────────────────────────

/// 记忆合并管理器
pub struct MemoryConsolidator {
    policy: ConsolidationPolicy,
    store: Option<Arc<dyn LongTermStore>>,
    last_consolidation: Arc<Mutex<DateTime<Utc>>>,
}

impl MemoryConsolidator {
    /// 创建新的合并管理器
    pub fn new(policy: ConsolidationPolicy) -> Self {
        Self {
            policy,
            store: None,
            last_consolidation: Arc::new(Mutex::new(Utc::now())),
        }
    }

    /// 设置长期存储后端
    pub fn with_store(mut self, store: Arc<dyn LongTermStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// 检查是否需要触发合并
    pub async fn should_consolidate(&self, buffer_len: usize) -> bool {
        if !self.policy.auto_consolidate || self.store.is_none() {
            return false;
        }

        let last = *self.last_consolidation.lock().await;
        let elapsed = Utc::now() - last;

        match self.policy.trigger {
            ConsolidationTrigger::Size(size) => buffer_len >= size,
            ConsolidationTrigger::Interval(secs) => {
                elapsed.num_seconds() >= secs as i64
            }
            ConsolidationTrigger::Both(size, secs) => {
                buffer_len >= size || elapsed.num_seconds() >= secs as i64
            }
        }
    }

    /// 执行合并：从缓冲中筛选条目，合并到长期存储
    pub async fn consolidate(
        &self,
        ctx: &LsContext,
        session_id: &str,
        buffer_items: &[MemoryItem],
    ) -> LsResult<ConsolidationResult> {
        let store = match &self.store {
            Some(s) => s,
            None => {
                return Ok(ConsolidationResult {
                    consolidated: 0,
                    skipped: buffer_items.len(),
                    errors: 0,
                });
            }
        };

        let now = Utc::now();
        let age_threshold = Duration::seconds(self.policy.max_age_seconds);

        // 筛选需要合并的条目（超过最大年龄 + 达到重要性阈值）
        let (to_consolidate, skipped): (Vec<&MemoryItem>, Vec<&MemoryItem>) = buffer_items
            .iter()
            .partition(|item| {
                let age = now - item.timestamp;
                let importance = importance_from_metadata(&item.metadata);
                age >= age_threshold && importance >= self.policy.min_importance
            });

        if to_consolidate.is_empty() {
            return Ok(ConsolidationResult {
                consolidated: 0,
                skipped: skipped.len(),
                errors: 0,
            });
        }

        let items: Vec<MemoryItem> = to_consolidate.into_iter().cloned().collect();
        let count = items.len();

        match store.store_batch(ctx, items).await {
            Ok(stored) => {
                *self.last_consolidation.lock().await = Utc::now();
                info!(
                    "consolidation: session={}, stored={}/{}",
                    session_id, stored, count
                );
                Ok(ConsolidationResult {
                    consolidated: stored,
                    skipped: skipped.len(),
                    errors: count - stored,
                })
            }
            Err(e) => {
                warn!("consolidation error: session={}, error={}", session_id, e);
                Ok(ConsolidationResult {
                    consolidated: 0,
                    skipped: skipped.len(),
                    errors: count,
                })
            }
        }
    }

    /// 获取配置引用
    pub fn policy(&self) -> &ConsolidationPolicy {
        &self.policy
    }
}

impl Default for MemoryConsolidator {
    fn default() -> Self {
        Self::new(ConsolidationPolicy::default())
    }
}

/// 合并操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationResult {
    /// 成功合并的条目数
    pub consolidated: usize,
    /// 跳过的条目数（未达条件）
    pub skipped: usize,
    /// 合并失败的条目数
    pub errors: usize,
}

impl ConsolidationResult {
    pub fn total(&self) -> usize {
        self.consolidated + self.skipped + self.errors
    }
}

/// 从元数据中提取重要性评分
fn importance_from_metadata(metadata: &serde_json::Value) -> Importance {
    metadata
        .get("importance")
        .and_then(|v| v.as_str())
        .map(Importance::from_str)
        .unwrap_or(Importance::Medium)
}

// ── 测试 ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MemoryItem;

    struct MockStore {
        stored: Arc<Mutex<Vec<MemoryItem>>>,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                stored: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl LongTermStore for MockStore {
        async fn store_batch(&self, _ctx: &LsContext, items: Vec<MemoryItem>) -> LsResult<usize> {
            let count = items.len();
            let mut stored = self.stored.lock().await;
            stored.extend(items);
            Ok(count)
        }

        async fn retrieve(&self, _ctx: &LsContext, _session_id: &str, _limit: usize) -> LsResult<Vec<MemoryItem>> {
            let stored = self.stored.lock().await;
            Ok(stored.clone())
        }

        async fn search(&self, _ctx: &LsContext, _session_id: &str, _query: &str, _limit: usize) -> LsResult<Vec<MemoryItem>> {
            let stored = self.stored.lock().await;
            Ok(stored.clone())
        }

        async fn clear_session(&self, _ctx: &LsContext, _session_id: &str) -> LsResult<usize> {
            let mut stored = self.stored.lock().await;
            let count = stored.len();
            stored.clear();
            Ok(count)
        }
    }

    #[test]
    fn test_consolidation_policy_defaults() {
        let policy = ConsolidationPolicy::default();
        assert_eq!(policy.max_age_seconds, 1800);
        assert!(policy.auto_consolidate);
    }

    #[tokio::test]
    async fn test_should_consolidate_no_store() {
        let consolidator = MemoryConsolidator::default();
        assert!(!consolidator.should_consolidate(100).await);
    }

    #[tokio::test]
    async fn test_should_consolidate_with_store_size_trigger() {
        let store = Arc::new(MockStore::new());
        let consolidator = MemoryConsolidator::new(ConsolidationPolicy {
            trigger: ConsolidationTrigger::Size(50),
            ..Default::default()
        }).with_store(store);

        assert!(!consolidator.should_consolidate(10).await);
        assert!(!consolidator.should_consolidate(49).await);
        assert!(consolidator.should_consolidate(50).await);
        assert!(consolidator.should_consolidate(100).await);
    }

    #[tokio::test]
    async fn test_consolidate_empty_buffer() {
        let store = Arc::new(MockStore::new());
        let consolidator = MemoryConsolidator::default().with_store(store);
        let ctx = LsContext::with_session(lingshu_core::LsId::new());

        let result = consolidator.consolidate(&ctx, "s1", &[]).await.unwrap();
        assert_eq!(result.consolidated, 0);
        assert_eq!(result.skipped, 0);
    }

    #[tokio::test]
    async fn test_consolidate_age_filter() {
        let store = Arc::new(MockStore::new());
        let consolidator = MemoryConsolidator::new(ConsolidationPolicy {
            max_age_seconds: 0, // 所有条目都超过 0 秒
            ..Default::default()
        }).with_store(store.clone());

        let ctx = LsContext::with_session(lingshu_core::LsId::new());

        let items = vec![
            MemoryItem::new("s1", "user", "旧消息"),
            MemoryItem::new("s1", "assistant", "旧回复"),
        ];

        let result = consolidator.consolidate(&ctx, "s1", &items).await.unwrap();
        assert_eq!(result.consolidated, 2);
        assert_eq!(result.skipped, 0);

        // 验证确实存入了 store
        let stored = store.stored.lock().await;
        assert_eq!(stored.len(), 2);
    }

    #[tokio::test]
    async fn test_consolidate_no_store_returns_skipped() {
        let consolidator = MemoryConsolidator::default();
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let items = vec![MemoryItem::new("s1", "user", "test")];

        let result = consolidator.consolidate(&ctx, "s1", &items).await.unwrap();
        assert_eq!(result.consolidated, 0);
        assert_eq!(result.skipped, 1);
    }

    #[test]
    fn test_importance_from_str() {
        assert_eq!(Importance::from_str("critical"), Importance::Critical);
        assert_eq!(Importance::from_str("high"), Importance::High);
        assert_eq!(Importance::from_str("medium"), Importance::Medium);
        assert_eq!(Importance::from_str("low"), Importance::Low);
        assert_eq!(Importance::from_str("unknown"), Importance::Medium);
    }

    #[test]
    fn test_importance_ordering() {
        assert!(Importance::Low < Importance::Medium);
        assert!(Importance::Medium < Importance::High);
        assert!(Importance::High < Importance::Critical);
    }
}
