//! AgentRuntime — v4.0 顶层运行时.
//!
//! 整合全系统核心组件：
//! - AgentManager — Agent 注册与生命周期
//! - AgentPool — Agent 复用池
//! - AgentFactory — Agent 创建工厂
//! - ToolRegistry — 工具注册与执行
//! - PluginRegistry — 插件管理
//! - SessionManager — 会话管理
//! - LifecycleManager — 生命周期

use std::sync::Arc;

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_plugin::PluginRegistry;
use lingshu_tool::ToolRegistry;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing::info;
use async_trait::async_trait;
use serde_json::Value;

use crate::agent_factory::LsAgentFactory;
use crate::agent_manager::AgentManager;
use crate::agent_pipeline::AgentPipeline;
use crate::agent_pool::{AgentPool, AgentPoolConfig};
use crate::lifecycle::{LifecycleManager, LifecycleState};
use crate::session::SessionManager;
use lingshu_traits::event_bus::EventBus;

/// WorkflowAccess — 工作流访问接口（避免循环依赖）.
#[async_trait]
pub trait WorkflowAccess: Send + Sync {
    /// 列出所有工作流.
    async fn list_workflows(&self) -> Vec<Value>;
    /// 执行工作流.
    async fn execute_workflow(&self, name: &str, ctx: LsContext, input: Value) -> LsResult<Value>;
    /// 查询工作流状态.
    async fn workflow_status(&self, name: &str) -> LsResult<Value>;
}

/// AgentRuntime 配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRuntimeConfig {
    /// Runtime 名称.
    pub name: String,
    /// Agent 池配置.
    pub pool_config: AgentPoolConfig,
    /// 会话默认 TTL（秒）.
    pub session_ttl_seconds: u64,
    /// 是否启用自动清理.
    pub enable_auto_cleanup: bool,
    /// 清理间隔（秒）.
    pub cleanup_interval_secs: u64,
}

impl Default for AgentRuntimeConfig {
    fn default() -> Self {
        Self {
            name: "lingshu".to_string(),
            pool_config: AgentPoolConfig::default(),
            session_ttl_seconds: 3600,
            enable_auto_cleanup: true,
            cleanup_interval_secs: 300,
        }
    }
}

/// AgentRuntime 内部状态.
struct AgentRuntimeInner {
    config: AgentRuntimeConfig,
    lifecycle: LifecycleManager,
    agent_manager: AgentManager,
    agent_pool: Option<AgentPool>,
    agent_factory: LsAgentFactory,
    tool_registry: Option<Arc<ToolRegistry>>,
    plugin_registry: Option<Arc<RwLock<PluginRegistry>>>,
    pipeline: Option<Arc<AgentPipeline>>,
    session_manager: SessionManager,
    workflow_access: Option<Arc<dyn WorkflowAccess>>,
    /// 可选的事件总线（用于实时事件推送）.
    event_bus: Option<Arc<dyn EventBus>>,
}

/// AgentRuntime — 顶层运行时.
#[derive(Clone)]
pub struct AgentRuntime {
    inner: Arc<RwLock<AgentRuntimeInner>>,
}

impl AgentRuntime {
    /// 创建新的 AgentRuntime.
    pub async fn new(config: AgentRuntimeConfig) -> LsResult<Self> {
        let session_mgr = SessionManager::new(config.session_ttl_seconds);
        let inner = AgentRuntimeInner {
            config,
            lifecycle: LifecycleManager::new(),
            agent_manager: AgentManager::new(),
            agent_pool: None,
            agent_factory: LsAgentFactory::new(),
            tool_registry: None,
            plugin_registry: None,
            pipeline: None,
            session_manager: session_mgr,
            workflow_access: None,
            event_bus: None,
        };
        let runtime = Self {
            inner: Arc::new(RwLock::new(inner)),
        };

        // 初始化生命周期
        {
            let inner = runtime.inner.write().await;
            let ctx = LsContext::with_session(LsId::new());
            inner.lifecycle.transition(&ctx, LifecycleState::Initializing)?;
        }

        info!("AgentRuntime created");
        Ok(runtime)
    }

    /// 启动 Runtime.
    pub async fn start(&self) -> LsResult<()> {
        let mut inner = self.inner.write().await;
        let ctx = LsContext::with_session(LsId::new());

        // 创建 Agent 池
        let factory_clone = LsAgentFactory::new();
        inner.agent_pool = Some(AgentPool::new(
            inner.config.pool_config.clone(),
            Box::new(factory_clone),
        ));

        inner.lifecycle.transition(&ctx, LifecycleState::Running)?;
        info!("AgentRuntime started");
        Ok(())
    }

    /// 停止 Runtime.
    pub async fn shutdown(&self) -> LsResult<()> {
        let inner = self.inner.write().await;
        let ctx = LsContext::with_session(LsId::new());

        inner.lifecycle.transition(&ctx, LifecycleState::ShuttingDown)?;
        inner.lifecycle.transition(&ctx, LifecycleState::Stopped)?;
        info!("AgentRuntime stopped");
        Ok(())
    }

    /// 获取生命周期状态.
    pub async fn lifecycle_state(&self) -> LsResult<LifecycleState> {
        let inner = self.inner.read().await;
        inner.lifecycle.current()
    }

    /// 获取会话管理器.
    pub async fn session_manager(&self) -> SessionManager {
        let inner = self.inner.read().await;
        inner.session_manager.clone()
    }

    /// 获取 Agent 池.
    pub async fn agent_pool(&self) -> Option<AgentPool> {
        let inner = self.inner.read().await;
        inner.agent_pool.clone()
    }

    /// 设置 ToolRegistry.
    pub async fn set_tool_registry(&self, registry: Arc<ToolRegistry>) {
        let mut inner = self.inner.write().await;
        inner.tool_registry = Some(registry);
        info!("ToolRegistry attached to AgentRuntime");
    }

    /// 设置 PluginRegistry.
    pub async fn set_plugin_registry(&self, registry: Arc<RwLock<PluginRegistry>>) {
        let mut inner = self.inner.write().await;
        inner.plugin_registry = Some(registry);
        info!("PluginRegistry attached to AgentRuntime");
    }

    /// 设置 AgentPipeline.
    pub async fn set_pipeline(&self, pipeline: Arc<AgentPipeline>) {
        let mut inner = self.inner.write().await;
        inner.pipeline = Some(pipeline.clone());
        inner.agent_factory = LsAgentFactory::new().with_default_pipeline(pipeline);
        info!("AgentPipeline attached to AgentRuntime");
    }

    /// 设置 WorkflowAccess.
    pub async fn set_workflow_access(&self, access: Arc<dyn WorkflowAccess>) {
        let mut inner = self.inner.write().await;
        inner.workflow_access = Some(access);
        info!("WorkflowAccess attached to AgentRuntime");
    }

    /// 设置 EventBus（用于实时事件推送）.
    pub async fn set_event_bus(&self, bus: Arc<dyn EventBus>) {
        let mut inner = self.inner.write().await;
        inner.event_bus = Some(bus);
        info!("EventBus attached to AgentRuntime");
    }

    /// 获取 EventBus 引用.
    pub async fn event_bus(&self) -> Option<Arc<dyn EventBus>> {
        let inner = self.inner.read().await;
        inner.event_bus.clone()
    }

    /// 检查是否配置了 WorkflowAccess.
    pub async fn has_workflow_access(&self) -> bool {
        let inner = self.inner.read().await;
        inner.workflow_access.is_some()
    }

    /// 列出所有工作流.
    pub async fn list_workflows(&self) -> Vec<Value> {
        let inner = self.inner.read().await;
        if let Some(ref access) = inner.workflow_access {
            access.list_workflows().await
        } else {
            Vec::new()
        }
    }

    /// 执行工作流.
    pub async fn execute_workflow(&self, name: &str, ctx: LsContext, input: Value) -> LsResult<Value> {
        let inner = self.inner.read().await;
        match &inner.workflow_access {
            Some(access) => access.execute_workflow(name, ctx, input).await,
            None => Err(LsError::NotFound("WorkflowAccess not configured".to_string())),
        }
    }

    /// 查询工作流状态.
    pub async fn workflow_status(&self, name: &str) -> LsResult<Value> {
        let inner = self.inner.read().await;
        match &inner.workflow_access {
            Some(access) => access.workflow_status(name).await,
            None => Err(LsError::NotFound("WorkflowAccess not configured".to_string())),
        }
    }

    /// 检查是否配置了 WorkflowAccess.
    /// 创建 Session.
    pub async fn create_session(&self, ctx: &LsContext) -> LsResult<()> {
        let inner = self.inner.read().await;
        inner.session_manager.create(ctx).await?;
        Ok(())
    }

    /// 获取配置.
    pub async fn config(&self) -> AgentRuntimeConfig {
        let inner = self.inner.read().await;
        inner.config.clone()
    }

    /// 注册 Agent 到 AgentManager.
    pub async fn register_agent(&self, agent_id: LsId, name: String, agent: Box<dyn lingshu_traits::agent::Agent>) {
        let inner = self.inner.read().await;
        inner.agent_manager.register(agent_id, name, agent).await;
    }

    /// 获取 Agent 池统计.
    pub async fn pool_stats(&self) -> LsResult<AgentPoolStats> {
        let inner = self.inner.read().await;
        match &inner.agent_pool {
            Some(pool) => Ok(pool.stats().await),
            None => Err(LsError::RuntimeState("agent pool not initialized".into())),
        }
    }

    /// 获取 Agent 数量.
    pub async fn agent_count(&self) -> usize {
        self.inner.read().await.agent_manager.count().await
    }

    /// 列出所有 Agent.
    pub async fn list_agents(&self) -> Vec<crate::agent_manager::AgentSummary> {
        self.inner.read().await.agent_manager.list().await
    }

    /// 获取 Agent 状态.
    pub async fn agent_status(&self, agent_id: &LsId) -> LsResult<lingshu_traits::agent::AgentStatus> {
        self.inner.read().await.agent_manager.status(agent_id).await
    }

    /// 移除 Agent.
    pub async fn remove_agent(&self, agent_id: &LsId) -> LsResult<()> {
        self.inner.read().await.agent_manager.remove(agent_id).await
    }

    /// 获取 ToolRegistry 引用.
    pub async fn tool_registry(&self) -> Option<Arc<lingshu_tool::ToolRegistry>> {
        self.inner.read().await.tool_registry.clone()
    }

    /// 获取 PluginRegistry 引用.
    pub async fn plugin_registry(&self) -> Option<Arc<tokio::sync::RwLock<lingshu_plugin::PluginRegistry>>> {
        self.inner.read().await.plugin_registry.clone()
    }

    /// 获取活跃会话数.
    pub async fn active_sessions(&self) -> u64 {
        self.inner.read().await.session_manager.active_count().await
    }

}

use crate::agent_pool::AgentPoolStats;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_runtime() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        let state = runtime.lifecycle_state().await.unwrap();
        assert_eq!(state, LifecycleState::Initializing);
    }

    #[tokio::test]
    async fn test_start_and_shutdown() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        runtime.start().await.unwrap();
        let state = runtime.lifecycle_state().await.unwrap();
        assert!(state.is_running());
        runtime.shutdown().await.unwrap();
        let state = runtime.lifecycle_state().await.unwrap();
        assert!(state.is_stopped());
    }

    #[tokio::test]
    async fn test_set_components() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        let pipeline = Arc::new(AgentPipeline::new());
        runtime.set_pipeline(pipeline).await;
        let registry = Arc::new(ToolRegistry::new());
        runtime.set_tool_registry(registry).await;
    }

    #[tokio::test]
    async fn test_create_session() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        let ctx = LsContext::with_session(LsId::new());
        runtime.create_session(&ctx).await.unwrap();
        let sm = runtime.session_manager().await;
        assert_eq!(sm.active_count().await, 1);
    }
}
