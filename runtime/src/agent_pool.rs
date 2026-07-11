//! AgentPool — 可复用的 Agent 池.
//!
//! 管理一组 Agent 实例，支持：
//! - 从池中获取/归还 Agent
//! - Agent 复用（避免反复创建）
//! - Agent 健康检查与自动清理
//! - 池容量动态调整

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};
use lingshu_traits::agent::{Agent, AgentStatus};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

/// Agent 工厂 trait — 创建 Agent 实例.
#[async_trait]
pub trait AgentFactory: Send + Sync {
    /// 创建指定名称的 Agent.
    async fn create(&self, name: &str, ctx: &LsContext) -> LsResult<Box<dyn Agent>>;

    /// 获取工厂支持的所有 Agent 名称.
    fn supported_agents(&self) -> Vec<String>;
}

/// 池中 Agent 条目.
#[allow(dead_code)]
struct PoolEntry {
    agent_id: LsId,
    name: String,
    agent: Box<dyn Agent>,
    status: AgentStatus,
    created_at: Instant,
    last_used: Instant,
    borrow_count: u64,
}

impl PoolEntry {
    fn new(agent_id: LsId, name: String, agent: Box<dyn Agent>) -> Self {
        let now = Instant::now();
        Self {
            agent_id,
            name,
            agent,
            status: AgentStatus::Idle,
            created_at: now,
            last_used: now,
            borrow_count: 0,
        }
    }
}

/// Agent 池配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPoolConfig {
    /// 最小空闲 Agent 数.
    pub min_idle: usize,
    /// 最大 Agent 数（包括已借出的）.
    pub max_total: usize,
    /// Agent 最大空闲时间（秒），超过将被清理.
    pub max_idle_secs: u64,
    /// Agent 最大借出时间（秒），超过将被强制回收.
    pub max_borrow_secs: u64,
    /// 健康检查间隔（秒）.
    pub health_check_interval_secs: u64,
}

impl Default for AgentPoolConfig {
    fn default() -> Self {
        Self {
            min_idle: 0,
            max_total: 32,
            max_idle_secs: 300,
            max_borrow_secs: 3600,
            health_check_interval_secs: 60,
        }
    }
}

/// Agent 池中的借出句柄.
/// Drop 时自动归还 Agent 到池中.
pub struct AgentHandle {
    agent_id: LsId,
    name: String,
    agent: Option<Box<dyn Agent>>,
    pool: AgentPool,
}

impl AgentHandle {
    /// 获取 Agent 的可变引用.
    pub fn agent_mut(&mut self) -> &mut Box<dyn Agent> {
        self.agent.as_mut().expect("agent already returned")
    }

    /// 获取 Agent 引用.
    pub fn agent(&self) -> &dyn Agent {
        self.agent.as_ref().expect("agent already returned")
    }

    /// 获取 Agent ID.
    pub fn agent_id(&self) -> LsId {
        self.agent_id
    }

    /// 获取 Agent 名称.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 手动归还 Agent 到池中.
    pub async fn return_to_pool(mut self) {
        if let Some(agent) = self.agent.take() {
            self.pool.return_agent(self.agent_id, agent).await;
        }
    }
}

impl Drop for AgentHandle {
    fn drop(&mut self) {
        if let Some(agent) = self.agent.take() {
            let pool = self.pool.clone();
            let agent_id = self.agent_id;
            tokio::spawn(async move {
                pool.return_agent(agent_id, agent).await;
            });
        }
    }
}

/// AgentPool — 线程安全的 Agent 池.
#[derive(Clone)]
pub struct AgentPool {
    config: Arc<AgentPoolConfig>,
    idle: Arc<Mutex<VecDeque<PoolEntry>>>,
    borrowed: Arc<RwLock<HashMap<LsId, Instant>>>,
    factory: Arc<Box<dyn AgentFactory>>,
    total: Arc<std::sync::atomic::AtomicUsize>,
}

impl AgentPool {
    /// 创建新的 Agent 池.
    pub fn new(config: AgentPoolConfig, factory: Box<dyn AgentFactory>) -> Self {
        Self {
            config: Arc::new(config),
            idle: Arc::new(Mutex::new(VecDeque::new())),
            borrowed: Arc::new(RwLock::new(HashMap::new())),
            factory: Arc::new(factory),
            total: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// 借出一个 Agent.
    ///
    /// 优先从空闲队列获取，如果不够则创建新的。
    pub async fn borrow(&self, ctx: &LsContext) -> LsResult<AgentHandle> {
        // 1. 尝试从空闲队列获取
        {
            let mut idle = self.idle.lock().await;
            if let Some(mut entry) = idle.pop_front() {
                entry.last_used = Instant::now();
                entry.borrow_count += 1;
                let agent_id = entry.agent_id;
                let name = entry.name.clone();
                self.borrowed.write().await.insert(agent_id, Instant::now());
                debug!(agent = %name, agent_id = %agent_id, "agent borrowed from pool");
                return Ok(AgentHandle {
                    agent_id,
                    name,
                    agent: Some(entry.agent),
                    pool: self.clone(),
                });
            }
        }

        // 2. 检查是否达到最大容量
        let total = self.total.load(std::sync::atomic::Ordering::Relaxed);
        if total >= self.config.max_total {
            // 尝试强制回收过期的借出 Agent
            self.reclaim_expired().await;

            let total = self.total.load(std::sync::atomic::Ordering::Relaxed);
            if total >= self.config.max_total {
                return Err(LsError::RuntimeState(format!(
                    "agent pool at capacity ({})",
                    self.config.max_total
                )));
            }
        }

        // 3. 创建新的 Agent
        self.total.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = "default".to_string();
        let agent = self.factory.create(&name, ctx).await?;
        let agent_id = agent.id();
        self.borrowed.write().await.insert(agent_id, Instant::now());
        info!(agent = %name, agent_id = %agent_id, "new agent created in pool");
        Ok(AgentHandle {
            agent_id,
            name,
            agent: Some(agent),
            pool: self.clone(),
        })
    }

    /// 按名称借出 Agent.
    pub async fn borrow_named(&self, name: &str, ctx: &LsContext) -> LsResult<AgentHandle> {
        // 先检查空闲队列中是否有匹配的
        {
            let mut idle = self.idle.lock().await;
            let pos = idle.iter().position(|e| e.name == name);
            if let Some(idx) = pos {
                let mut entry = idle.remove(idx).unwrap();
                entry.last_used = Instant::now();
                entry.borrow_count += 1;
                let agent_id = entry.agent_id;
                let name = entry.name.clone();
                self.borrowed.write().await.insert(agent_id, Instant::now());
                return Ok(AgentHandle {
                    agent_id,
                    name,
                    agent: Some(entry.agent),
                    pool: self.clone(),
                });
            }
        }

        // 创建新的
        let total = self.total.load(std::sync::atomic::Ordering::Relaxed);
        if total >= self.config.max_total {
            self.reclaim_expired().await;
            let total = self.total.load(std::sync::atomic::Ordering::Relaxed);
            if total >= self.config.max_total {
                return Err(LsError::RuntimeState(format!(
                    "agent pool at capacity ({})",
                    self.config.max_total
                )));
            }
        }

        self.total.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let agent = self.factory.create(name, ctx).await?;
        let agent_id = agent.id();
        self.borrowed.write().await.insert(agent_id, Instant::now());
        Ok(AgentHandle {
            agent_id,
            name: name.to_string(),
            agent: Some(agent),
            pool: self.clone(),
        })
    }

    /// 归还 Agent 到池中.
    async fn return_agent(&self, agent_id: LsId, agent: Box<dyn Agent>) {
        // 从借出列表移除
        self.borrowed.write().await.remove(&agent_id);

        // 健康检查：如果 Agent 状态异常，不归还
        let status = agent.status(LsContext::with_session(LsId::new())).await;
        match status {
            Ok(AgentStatus::Failed) | Ok(AgentStatus::Completed) => {
                debug!(agent_id = %agent_id, "agent not reusable, dropping");
                self.total.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            _ => {}
        }

        // 归还到空闲队列
        let mut idle = self.idle.lock().await;
        if idle.len() >= self.config.max_total / 2 {
            // 空闲队列已满，丢弃多余的
            debug!(agent_id = %agent_id, "idle pool full, dropping agent");
            self.total.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            return;
        }

        idle.push_back(PoolEntry::new(agent_id, "default".to_string(), agent));
        debug!(agent_id = %agent_id, idle = idle.len(), "agent returned to pool");
    }

    /// 回收过期的借出 Agent.
    async fn reclaim_expired(&self) {
        let now = Instant::now();
        let max_borrow = std::time::Duration::from_secs(self.config.max_borrow_secs);
        let expired_ids: Vec<LsId> = {
            let borrowed = self.borrowed.read().await;
            borrowed
                .iter()
                .filter(|(_, &start)| now.duration_since(start) > max_borrow)
                .map(|(id, _)| *id)
                .collect()
        };

        for id in expired_ids {
            self.borrowed.write().await.remove(&id);
            self.total.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            warn!(agent_id = %id, "reclaimed expired borrowed agent");
        }
    }

    /// 清理空闲队列中过期的 Agent.
    pub async fn clean_idle(&self) -> usize {
        let max_idle = std::time::Duration::from_secs(self.config.max_idle_secs);
        let now = Instant::now();
        let mut idle = self.idle.lock().await;
        let before = idle.len();
        idle.retain(|e| now.duration_since(e.last_used) < max_idle);
        let removed = before - idle.len();
        self.total.fetch_sub(removed as usize, std::sync::atomic::Ordering::Relaxed);

        // 确保达到 min_idle
        let shortfall = self.config.min_idle.saturating_sub(idle.len());
        if shortfall > 0 {
            drop(idle);
            for _ in 0..shortfall {
                if let Ok(handle) = self.borrow(&LsContext::with_session(LsId::new())).await {
                    handle.return_to_pool().await;
                }
            }
        }

        info!(removed = removed, "cleaned idle agents from pool");
        removed
    }

    /// 获取池统计信息.
    pub async fn stats(&self) -> AgentPoolStats {
        let idle = self.idle.lock().await.len();
        let borrowed = self.borrowed.read().await.len();
        let total = self.total.load(std::sync::atomic::Ordering::Relaxed);
        AgentPoolStats {
            idle,
            borrowed,
            total,
            max_total: self.config.max_total,
        }
    }

    /// 获取空闲 Agent 数量.
    pub async fn idle_count(&self) -> usize {
        self.idle.lock().await.len()
    }

    /// 获取已借出 Agent 数量.
    pub async fn borrowed_count(&self) -> usize {
        self.borrowed.read().await.len()
    }

    /// 获取总 Agent 数量.
    pub fn total_count(&self) -> usize {
        self.total.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Agent 池统计信息.
#[derive(Debug, Clone)]
pub struct AgentPoolStats {
    pub idle: usize,
    pub borrowed: usize,
    pub total: usize,
    pub max_total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_traits::agent::{AgentOutput, AgentSnapshot};

    struct DummyAgent {
        id: LsId,
    }

    #[async_trait]
    impl Agent for DummyAgent {
        fn id(&self) -> LsId { self.id }
        async fn run(&mut self, _ctx: LsContext, _input: serde_json::Value) -> LsResult<AgentOutput> {
            Ok(AgentOutput {
                agent_id: self.id,
                status: AgentStatus::Completed,
                data: Some(serde_json::Value::Null),
                error: None,
            })
        }
        async fn pause(&mut self, _ctx: LsContext) -> LsResult<()> { Ok(()) }
        async fn resume(&mut self, _ctx: LsContext) -> LsResult<()> { Ok(()) }
        async fn cancel(&mut self, _ctx: LsContext) -> LsResult<()> { Ok(()) }
        async fn snapshot(&self, _ctx: LsContext) -> LsResult<AgentSnapshot> {
            Ok(AgentSnapshot {
                agent_id: self.id,
                status: AgentStatus::Idle,
                context: LsContext::with_session(LsId::new()),
                state: Vec::new(),
                created_at: chrono::Utc::now(),
            })
        }
        async fn restore(&mut self, _ctx: LsContext, _snap: AgentSnapshot) -> LsResult<()> { Ok(()) }
        async fn status(&self, _ctx: LsContext) -> LsResult<AgentStatus> { Ok(AgentStatus::Idle) }
    }

    struct TestFactory;
    #[async_trait]
    impl AgentFactory for TestFactory {
        async fn create(&self, _name: &str, _ctx: &LsContext) -> LsResult<Box<dyn Agent>> {
            Ok(Box::new(DummyAgent { id: LsId::new() }))
        }
        fn supported_agents(&self) -> Vec<String> {
            vec!["test".to_string()]
        }
    }

    #[tokio::test]
    async fn test_borrow_and_return() {
        let pool = AgentPool::new(AgentPoolConfig::default(), Box::new(TestFactory));
        let ctx = LsContext::with_session(LsId::new());
        let handle = pool.borrow(&ctx).await.unwrap();
        assert_eq!(pool.borrowed_count().await, 1);
        handle.return_to_pool().await;
        assert_eq!(pool.borrowed_count().await, 0);
        assert_eq!(pool.idle_count().await, 1);
    }

    #[tokio::test]
    async fn test_idle_reuse() {
        let pool = AgentPool::new(AgentPoolConfig::default(), Box::new(TestFactory));
        let ctx = LsContext::with_session(LsId::new());
        let h1 = pool.borrow(&ctx).await.unwrap();
        let id1 = h1.agent_id();
        h1.return_to_pool().await;

        let h2 = pool.borrow(&ctx).await.unwrap();
        assert_eq!(h2.agent_id(), id1, "should reuse same agent");
        h2.return_to_pool().await;
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let pool = AgentPool::new(AgentPoolConfig::default(), Box::new(TestFactory));
        let stats = pool.stats().await;
        assert_eq!(stats.idle, 0);
        assert_eq!(stats.borrowed, 0);
    }

    #[tokio::test]
    async fn test_clean_idle() {
        let pool = AgentPool::new(
            AgentPoolConfig {
                max_idle_secs: 0,
                ..Default::default()
            },
            Box::new(TestFactory),
        );
        let ctx = LsContext::with_session(LsId::new());
        let h = pool.borrow(&ctx).await.unwrap();
        h.return_to_pool().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let cleaned = pool.clean_idle().await;
        assert_eq!(cleaned, 1);
    }
}
