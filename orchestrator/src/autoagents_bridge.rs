#![allow(unused_imports, dead_code)]
//! AutoAgents Bridge — 多 Agent 编排框架集成
//!
//! 将 [AutoAgents](https://github.com/liquidos-ai/AutoAgents) 的
//! ReAct agent + 结构化工具调用能力引入 Lingshu 编排器。
//!
//! ## Feature Gate
//! `#[cfg(feature = "autoagents")]` — 需在 `Cargo.toml` 中启用：
//! ```toml
//! autoagents = { git = "https://github.com/liquidos-ai/AutoAgents.git" }
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::agent::{Agent as LsAgent, AgentOutput, AgentSnapshot, AgentStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

use crate::orchestrator::{DelegationResult, Orchestrator, OrchestratorConfig, TeamConfig};
use crate::registry::{AgentCapability, AgentInfo, AgentRegistry};
use crate::scheduler::ScheduleStrategy;

// ── 公共类型（无 feature gate）────────────────────────

/// ReAct Agent 配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActConfig {
    /// 最大推理步数
    pub max_steps: u32,
    /// 温度参数
    pub temperature: f64,
    /// 是否启用工具调用
    pub enable_tools: bool,
    /// 允许的工具名称列表
    pub allowed_tools: Vec<String>,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            temperature: 0.7,
            enable_tools: true,
            allowed_tools: Vec::new(),
        }
    }
}

/// AutoAgents Crew 配置 — 团队/编队定义.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewConfig {
    /// 编队名称
    pub name: String,
    /// 编队描述
    pub description: String,
    /// 所需的智能体能力
    pub required_capabilities: Vec<String>,
    /// ReAct 配置
    pub react_config: ReActConfig,
    /// 是否启用平行执行
    pub parallel_execution: bool,
}

// ── AutoAgents 真实实现 (feature = "autoagents") ────

/// AutoAgentsOrchestrator — 将 AutoAgents 的 ReAct agent 作为 Lingshu 智能体接入.
///
/// 包装 AutoAgents 的多 Agent 编排功能，通过 Lingshu 的 `Agent` trait 统一接口暴露。
/// 支持团队/编队创建、任务委派和 ReAct 循环执行。
#[cfg(feature = "autoagents")]
pub struct AutoAgentsOrchestrator {
    /// Lingshu 编排器实例
    orchestrator: Orchestrator,
    /// 编队配置缓存
    crews: tokio::sync::RwLock<HashMap<String, CrewConfig>>,
    /// AutoAgents LLM 提供者（可选，用于未来集成）
    llm: Option<std::sync::Arc<dyn autoagents::llm::LLMProvider>>,
}

#[cfg(feature = "autoagents")]
impl AutoAgentsOrchestrator {
    /// 创建 AutoAgents 编排器.
    pub fn new(config: OrchestratorConfig) -> Self {
        Self {
            orchestrator: Orchestrator::new(config),
            crews: tokio::sync::RwLock::new(HashMap::new()),
            llm: None,
        }
    }

    /// 初始化 LLM 提供者（未来将用于 AgentBuilder 集成）.
    pub async fn init_engine(&mut self, _llm_endpoint: &str, _api_key: &str) -> LsResult<()> {
        // 注意: autoagents 上游 API 已重构 (v0.4.0)
        // 旧 API (ReActEngine) 已移除，新 API 使用 AgentBuilder + BaseAgent + AgentExecutor
        // TODO: 使用 AgentBuilder::new().llm(llm).build() 重构集成
        info!("AutoAgents bridge: LLM provider registration pending (API migration)");
        Ok(())
    }

    /// 创建编队（相当于 Lingshu Team）.
    pub async fn create_crew(&self, config: CrewConfig) -> LsResult<()> {
        let team_config = TeamConfig {
            name: config.name.clone(),
            description: Some(config.description.clone()),
            required_capabilities: config.required_capabilities.clone(),
            strategy: if config.parallel_execution {
                ScheduleStrategy::RoundRobin
            } else {
                ScheduleStrategy::Sequential
            },
        };

        self.orchestrator.create_team(team_config).await?;

        let mut crews = self.crews.write().await;
        crews.insert(config.name.clone(), config.clone());
        info!(crew = %config.name, "AutoAgents crew created");
        Ok(())
    }

    /// 向编队委派 ReAct 任务.
    pub async fn delegate_react(
        &self,
        crew_name: &str,
        task: serde_json::Value,
        ctx: &LsContext,
    ) -> LsResult<DelegationResult> {
        info!(
            crew = %crew_name,
            "AutoAgents bridge: delegating via standard orchestrator (ReAct integration pending)"
        );
        // 暂回退到标准编排器
        // TODO: 使用 AgentExecutor + TurnEngine 实现 ReAct 循环
        self.orchestrator.delegate(crew_name, task, ctx).await
    }

    /// 注册一个 AutoAgents 智能体到 Lingshu 注册表.
    pub async fn register_agent(
        &self,
        agent_id: LsId,
        name: &str,
        capabilities: Vec<&str>,
    ) -> LsResult<()> {
        let info = AgentInfo {
            agent_id,
            name: name.to_string(),
            version: "1.0".into(),
            capabilities: capabilities
                .into_iter()
                .map(|c| AgentCapability {
                    name: c.to_string(),
                    version: "1.0".into(),
                    description: None,
                })
                .collect(),
            status: AgentStatus::Idle,
            created_at: chrono::Utc::now(),
            last_heartbeat: chrono::Utc::now(),
            tags: HashMap::new(),
        };
        self.orchestrator.registry.register(info).await
    }

    /// 获取内部编排器引用.
    pub fn orchestrator(&self) -> &Orchestrator {
        &self.orchestrator
    }

    /// 列出所有编队.
    pub async fn list_crews(&self) -> Vec<CrewConfig> {
        self.crews.read().await.values().cloned().collect()
    }
}

// ── 非 autoagents 编译时的桩 ─────────────────────────

/// 非 autoagents 编译时的空实现.
#[cfg(not(feature = "autoagents"))]
pub struct AutoAgentsOrchestrator;

#[cfg(not(feature = "autoagents"))]
impl AutoAgentsOrchestrator {
    /// 创建空实例.
    pub fn new(_config: OrchestratorConfig) -> Self {
        Self
    }

    /// 初始化引擎（返回 Unsupported 错误）.
    pub async fn init_engine(
        &mut self,
        _llm_endpoint: &str,
        _api_key: &str,
    ) -> LsResult<()> {
        Err(LsError::NotImplemented(
            "autoagents feature not enabled".into(),
        ))
    }

    /// 创建编队.
    pub async fn create_crew(&self, _config: CrewConfig) -> LsResult<()> {
        Err(LsError::NotImplemented(
            "autoagents feature not enabled".into(),
        ))
    }

    /// 委派 ReAct 任务.
    pub async fn delegate_react(
        &self,
        _crew_name: &str,
        _task: serde_json::Value,
        _ctx: &LsContext,
    ) -> LsResult<DelegationResult> {
        Err(LsError::NotImplemented(
            "autoagents feature not enabled".into(),
        ))
    }

    /// 注册智能体.
    pub async fn register_agent(
        &self,
        _agent_id: LsId,
        _name: &str,
        _capabilities: Vec<&str>,
    ) -> LsResult<()> {
        Err(LsError::NotImplemented(
            "autoagents feature not enabled".into(),
        ))
    }

    /// 列出编队.
    pub async fn list_crews(&self) -> Vec<CrewConfig> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crew_config_serde() {
        let config = CrewConfig {
            name: "test-crew".into(),
            description: "Test crew".into(),
            required_capabilities: vec!["code".into(), "reasoning".into()],
            react_config: ReActConfig::default(),
            parallel_execution: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CrewConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test-crew");
        assert_eq!(deserialized.required_capabilities.len(), 2);
    }

    #[tokio::test]
    async fn test_autoagents_noop_stub() {
        let orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());
        let err = orch
            .delegate_react("test", serde_json::json!({"task": "x"}), &LsContext::with_session(LsId::new()))
            .await;
        if cfg!(not(feature = "autoagents")) {
            assert!(err.is_err());
        }
    }
}

#[cfg(test)]
#[cfg(not(feature = "autoagents"))]
mod stub_tests {
    use super::*;
    use lingshu_core::{LsContext, LsId};

    #[test]
    fn test_stub_react_config_default() {
        let config = ReActConfig::default();
        assert_eq!(config.max_steps, 10);
        assert_eq!(config.temperature, 0.7);
        assert!(config.enable_tools);
    }

    #[test]
    fn test_stub_crew_config_serde() {
        let config = CrewConfig {
            name: "stub-crew".into(),
            description: "Stub test".into(),
            required_capabilities: vec!["test".into()],
            react_config: ReActConfig::default(),
            parallel_execution: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CrewConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "stub-crew");
    }

    #[tokio::test]
    async fn test_stub_register_agent_unsupported() {
        let orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());
        let err = orch
            .register_agent(LsId::new(), "stub", vec!["test"])
            .await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("autoagents feature not enabled"));
    }

    #[tokio::test]
    async fn test_stub_list_crews_empty() {
        let orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());
        let crews = orch.list_crews().await;
        assert!(crews.is_empty());
    }

    #[tokio::test]
    async fn test_stub_delegate_react_unsupported() {
        let orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());
        let ctx = LsContext::with_session(LsId::new());
        let err = orch
            .delegate_react("crew", serde_json::json!({"x": 1}), &ctx)
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_stub_create_crew_unsupported() {
        let orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());
        let err = orch
            .create_crew(CrewConfig {
                name: "test".into(),
                description: String::new(),
                required_capabilities: vec![],
                react_config: ReActConfig::default(),
                parallel_execution: false,
            })
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_stub_init_engine_unsupported() {
        let mut orch = AutoAgentsOrchestrator::new(OrchestratorConfig::default());
        let err = orch.init_engine("http://localhost", "key").await;
        assert!(err.is_err());
    }
}
