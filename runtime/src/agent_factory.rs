//! AgentFactory — Agent 工厂，从配置创建多种 Agent 实例.
//!
//! 支持：
//! - PipelineAgent（基于 AgentPipeline）
//! - Custom Agent（通过构造闭包直接注册）
//! - Agent 配置模板

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};

use crate::agent_pipeline::{AgentPipeline, PipelineAgent};
use crate::agent_pool::AgentFactory;
use lingshu_traits::agent::Agent;
use tokio::sync::RwLock;
use tracing::info;

/// Agent 注册信息.
pub struct AgentRegistration {
    /// Agent 名称.
    pub name: String,
    /// Agent 描述.
    pub description: String,
    /// 创建函数 — 每次调用返回新的 Agent 实例.
    pub factory: Box<dyn Fn() -> Box<dyn Agent> + Send + Sync>,
}

/// 增强版 AgentFactory — 支持多种 Agent 类型注册.
pub struct LsAgentFactory {
    /// 已注册的 Agent 构造器.
    registrations: RwLock<HashMap<String, AgentRegistration>>,
    /// 默认的 PipelineAgent 构造器（当没有注册匹配时使用）.
    default_pipeline: Option<Arc<AgentPipeline>>,
}

impl LsAgentFactory {
    /// 创建新的 Agent 工厂.
    pub fn new() -> Self {
        Self {
            registrations: RwLock::new(HashMap::new()),
            default_pipeline: None,
        }
    }

    /// 设置默认 PipelineAgent 构造器.
    pub fn with_default_pipeline(mut self, pipeline: Arc<AgentPipeline>) -> Self {
        self.default_pipeline = Some(pipeline);
        self
    }

    /// 注册一个 Agent 类型.
    pub async fn register(&self, registration: AgentRegistration) {
        let name = registration.name.clone();
        self.registrations.write().await.insert(name.clone(), registration);
        info!(agent = %name, "agent registered in factory");
    }

    /// 注册一个自定义 Agent（通过构造闭包）.
    pub async fn register_factory(&self, name: &str, factory: Box<dyn Fn() -> Box<dyn Agent> + Send + Sync>) {
        let name = name.to_string();
        self.registrations.write().await.insert(name.clone(), AgentRegistration {
            name: name.clone(),
            description: format!("Custom agent '{}'", name),
            factory,
        });
        info!(agent = %name, "custom agent factory registered");
    }

    /// 检查是否支持指定名称的 Agent.
    pub async fn supports(&self, name: &str) -> bool {
        let regs = self.registrations.read().await;
        regs.contains_key(name) || self.default_pipeline.is_some()
    }

    /// 获取所有已注册的 Agent 名称.
    pub async fn registered_names(&self) -> Vec<String> {
        self.registrations.read().await.keys().cloned().collect()
    }

    /// 创建不带默认 Pipeline 的浅拷贝（用于 AgentPool）.
    pub fn shallow_clone(&self) -> Self {
        Self {
            registrations: RwLock::new(HashMap::new()),
            default_pipeline: self.default_pipeline.clone(),
        }
    }
}

impl Default for LsAgentFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentFactory for LsAgentFactory {
    async fn create(&self, name: &str, _ctx: &LsContext) -> LsResult<Box<dyn Agent>> {
        // 1. 优先查找已注册的 Agent
        {
            let regs = self.registrations.read().await;
            if let Some(reg) = regs.get(name) {
                info!(agent = %name, "creating registered agent");
                return Ok((reg.factory)());
            }
        }

        // 2. 如果有默认 Pipeline，使用 PipelineAgent
        if let Some(pipeline) = &self.default_pipeline {
            info!(agent = %name, "creating pipeline agent");
            let id = LsId::new();
            return Ok(Box::new(PipelineAgent::new(id, name, pipeline.clone())));
        }

        Err(LsError::NotFound(format!(
            "no agent factory registered for '{name}', and no default pipeline configured"
        )))
    }

    fn supported_agents(&self) -> Vec<String> {
        vec!["default".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_traits::agent::{AgentOutput, AgentSnapshot, AgentStatus};

    struct EchoAgent;
    #[async_trait]
    impl Agent for EchoAgent {
        fn id(&self) -> LsId { LsId::new() }
        async fn run(&mut self, _ctx: LsContext, input: serde_json::Value) -> LsResult<AgentOutput> {
            Ok(AgentOutput {
                agent_id: LsId::new(),
                status: AgentStatus::Completed,
                data: Some(input),
                error: None,
            })
        }
        async fn pause(&mut self, _ctx: LsContext) -> LsResult<()> { Ok(()) }
        async fn resume(&mut self, _ctx: LsContext) -> LsResult<()> { Ok(()) }
        async fn cancel(&mut self, _ctx: LsContext) -> LsResult<()> { Ok(()) }
        async fn snapshot(&self, _ctx: LsContext) -> LsResult<AgentSnapshot> {
            Ok(AgentSnapshot {
                agent_id: LsId::new(),
                status: AgentStatus::Idle,
                context: LsContext::with_session(LsId::new()),
                state: Vec::new(),
                created_at: chrono::Utc::now(),
            })
        }
        async fn restore(&mut self, _ctx: LsContext, _snap: AgentSnapshot) -> LsResult<()> { Ok(()) }
        async fn status(&self, _ctx: LsContext) -> LsResult<AgentStatus> { Ok(AgentStatus::Idle) }
    }

    #[tokio::test]
    async fn test_create_registered_agent() {
        let factory = LsAgentFactory::new();
        factory.register_factory("echo", Box::new(|| Box::new(EchoAgent))).await;
        let ctx = LsContext::with_session(LsId::new());
        let agent = factory.create("echo", &ctx).await.unwrap();
        let id = agent.id();
        assert!(!id.is_nil());
    }

    #[tokio::test]
    async fn test_create_unregistered_fails() {
        let factory = LsAgentFactory::new();
        let ctx = LsContext::with_session(LsId::new());
        let result = factory.create("nonexistent", &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_supports() {
        let factory = LsAgentFactory::new();
        factory.register_factory("echo", Box::new(|| Box::new(EchoAgent))).await;
        assert!(factory.supports("echo").await);
        assert!(!factory.supports("unknown").await);
    }

    #[tokio::test]
    async fn test_registered_names() {
        let factory = LsAgentFactory::new();
        factory.register_factory("echo", Box::new(|| Box::new(EchoAgent))).await;
        let names = factory.registered_names().await;
        assert!(names.contains(&"echo".to_string()));
    }
}
