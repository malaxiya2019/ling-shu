#![allow(unused_imports, dead_code)]
//! Loong Adapter — 轻量可扩展 Agent 基础设施集成
//!
//! 将 [loong](https://github.com/eastreams/loong) 的
//! 轻量 Agent 基础设施接入 Lingshu 编排系统。
//!
//! ## Feature Gate
//! `#[cfg(feature = "loong")]` — 需在 `Cargo.toml` 中启用：
//! ```toml
//! loong = { git = "https://github.com/eastreams/loong.git" }
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::agent::{Agent as LsAgent, AgentOutput, AgentSnapshot, AgentStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::orchestrator::{Orchestrator, OrchestratorConfig};
use crate::registry::{AgentCapability, AgentInfo, AgentRegistry};

// ── 公共类型 ─────────────────────────────────────────

/// Loong Agent 配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoongAgentConfig {
    /// Agent 名称
    pub name: String,
    /// Agent 描述
    pub description: String,
    /// 能力列表
    pub capabilities: Vec<String>,
    /// 模型配置
    pub model: String,
    /// 系统提示词
    pub system_prompt: Option<String>,
    /// 最大 token 数
    pub max_tokens: u32,
}

impl Default for LoongAgentConfig {
    fn default() -> Self {
        Self {
            name: "loong-agent".into(),
            description: String::new(),
            capabilities: Vec::new(),
            model: "default".into(),
            system_prompt: None,
            max_tokens: 4096,
        }
    }
}

// ── Loong 真实实现 (feature = "loong") ──────────────

/// LoongAdapter — 将 loong 轻量 Agent 接入 Lingshu.
///
/// 使用 loong 的 Agent 基础设施创建和管理轻量智能体。
/// 适合不需要完整编排能力的简单 Agent 场景。
#[cfg(feature = "loong")]
pub struct LoongAdapter {
    /// loong 运行时
    runtime: Arc<RwLock<loong::Runtime>>,
    /// 注册的 loong agent 缓存
    agents: Arc<RwLock<HashMap<LsId, loong::AgentHandle>>>,
    /// Lingshu 注册表引用
    registry: Option<Arc<AgentRegistry>>,
}

#[cfg(feature = "loong")]
impl LoongAdapter {
    /// 创建 LoongAdapter.
    pub fn new() -> Self {
        let runtime = Arc::new(RwLock::new(loong::Runtime::new()));
        Self {
            runtime,
            agents: Arc::new(RwLock::new(HashMap::new())),
            registry: None,
        }
    }

    /// 绑定 Lingshu AgentRegistry.
    pub fn with_registry(mut self, registry: Arc<AgentRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// 创建并注册一个 loong agent.
    pub async fn create_agent(&self, config: LoongAgentConfig) -> LsResult<LsId> {
        let agent_id = LsId::new();

        // 在 loong 运行时中创建 agent
        let handle = {
            let mut rt = self
                .runtime
                .write()
                .await;
            rt.create_agent(
                &config.name,
                loong::AgentSpec {
                    description: config.description.clone(),
                    model: config.model.clone(),
                    system_prompt: config.system_prompt.clone(),
                    max_tokens: config.max_tokens,
                },
            )
            .map_err(|e| LsError::Internal(format!("loong create_agent failed: {e}")))?
        };

        // 缓存 handle
        {
            let mut agents = self.agents.write().await;
            agents.insert(agent_id, handle);
        }

        // 注册到 Lingshu 注册表
        if let Some(registry) = &self.registry {
            let info = AgentInfo {
                agent_id,
                name: config.name.clone(),
                version: "1.0".into(),
                capabilities: config
                    .capabilities
                    .iter()
                    .map(|c| AgentCapability {
                        name: c.clone(),
                        version: "1.0".into(),
                        description: None,
                    })
                    .collect(),
                status: AgentStatus::Idle,
                created_at: chrono::Utc::now(),
                last_heartbeat: chrono::Utc::now(),
                tags: HashMap::from([
                    ("source".into(), "loong".into()),
                    ("model".into(), config.model.clone()),
                ]),
            };
            registry.register(info).await?;
        }

        info!(
            agent_id = %agent_id,
            name = %config.name,
            "loong agent created and registered"
        );
        Ok(agent_id)
    }

    /// 执行 agent.
    pub async fn run_agent(
        &self,
        agent_id: &LsId,
        input: serde_json::Value,
    ) -> LsResult<AgentOutput> {
        let handle = {
            let agents = self.agents.read().await;
            agents
                .get(agent_id)
                .cloned()
                .ok_or_else(|| LsError::NotFound(format!("loong agent {agent_id}")))?
        };

        let result = handle
            .run(input.to_string())
            .await
            .map_err(|e| LsError::Internal(format!("loong run failed: {e}")))?;

        Ok(AgentOutput {
            agent_id: *agent_id,
            status: AgentStatus::Completed,
            data: Some(serde_json::json!({ "output": result })),
            error: None,
        })
    }

    /// 停止 agent.
    pub async fn stop_agent(&self, agent_id: &LsId) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        if let Some(handle) = agents.remove(agent_id) {
            handle
                .stop()
                .await
                .map_err(|e| LsError::Internal(format!("loong stop failed: {e}")))?;
            info!(agent_id = %agent_id, "loong agent stopped");
        }
        Ok(())
    }

    /// 列出所有 loong agent.
    pub async fn list_agents(&self) -> Vec<LsId> {
        self.agents.read().await.keys().copied().collect()
    }

    /// 获取 loong 运行时状态.
    pub async fn runtime_status(&self) -> LsResult<serde_json::Value> {
        let rt = self.runtime.read().await;
        Ok(serde_json::json!({
            "agent_count": self.agents.read().await.len(),
            "runtime_health": rt.health().map(|h| h.to_string()).unwrap_or_default(),
        }))
    }
}

#[cfg(feature = "loong")]
impl Default for LoongAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ── 非 loong 编译时的桩 ─────────────────────────────

/// 非 loong 编译时的空实现.
#[cfg(not(feature = "loong"))]
pub struct LoongAdapter;

#[cfg(not(feature = "loong"))]
impl LoongAdapter {
    /// 创建空实例.
    pub fn new() -> Self {
        Self
    }

    /// 绑定注册表.
    pub fn with_registry(self, _registry: Arc<AgentRegistry>) -> Self {
        self
    }

    /// 创建 agent.
    pub async fn create_agent(&self, _config: LoongAgentConfig) -> LsResult<LsId> {
        Err(LsError::NotImplemented("loong feature not enabled".into()))
    }

    /// 执行 agent.
    pub async fn run_agent(
        &self,
        _agent_id: &LsId,
        _input: serde_json::Value,
    ) -> LsResult<AgentOutput> {
        Err(LsError::NotImplemented("loong feature not enabled".into()))
    }

    /// 停止 agent.
    pub async fn stop_agent(&self, _agent_id: &LsId) -> LsResult<()> {
        Err(LsError::NotImplemented("loong feature not enabled".into()))
    }

    /// 列出 agents.
    pub async fn list_agents(&self) -> Vec<LsId> {
        Vec::new()
    }

    /// 运行时状态.
    pub async fn runtime_status(&self) -> LsResult<serde_json::Value> {
        Ok(serde_json::json!({"agent_count": 0, "runtime_health": "stub"}))
    }
}

#[cfg(not(feature = "loong"))]
impl Default for LoongAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_loong_config_serde() {
        let config = LoongAgentConfig {
            name: "loong-1".into(),
            description: "Test loong agent".into(),
            capabilities: vec!["code".into(), "reasoning".into()],
            model: "gpt-4".into(),
            system_prompt: Some("You are a helpful assistant.".into()),
            max_tokens: 8192,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: LoongAgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "loong-1");
        assert_eq!(deserialized.capabilities.len(), 2);
        assert_eq!(deserialized.max_tokens, 8192);
    }

    #[tokio::test]
    async fn test_loong_stub() {
        let adapter = LoongAdapter::new();
        let err = adapter
            .run_agent(&LsId::new(), serde_json::json!({"task": "test"}))
            .await;
        if cfg!(not(feature = "loong")) {
            assert!(err.is_err());
        }
    }

    #[tokio::test]
    async fn test_loong_stub_list() {
        let adapter = LoongAdapter::new();
        let agents = adapter.list_agents().await;
        assert!(agents.is_empty());
    }
}

#[cfg(test)]
#[cfg(not(feature = "loong"))]
mod stub_tests {
    use super::*;
    use lingshu_core::{LsContext, LsId};
    use std::sync::Arc;
    use crate::registry::AgentRegistry;

    #[test]
    fn test_stub_config_serde() {
        let config = LoongAgentConfig {
            name: "stub-agent".into(),
            description: "Stub test".into(),
            capabilities: vec!["a".into()],
            model: "default".into(),
            system_prompt: None,
            max_tokens: 2048,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: LoongAgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "stub-agent");
        assert_eq!(deserialized.max_tokens, 2048);
    }

    #[tokio::test]
    async fn test_stub_create_agent_unsupported() {
        let adapter = LoongAdapter::new();
        let err = adapter
            .create_agent(LoongAgentConfig::default())
            .await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("loong feature not enabled"));
    }

    #[tokio::test]
    async fn test_stub_run_agent_unsupported() {
        let adapter = LoongAdapter::new();
        let err = adapter
            .run_agent(&LsId::new(), serde_json::json!({"x": 1}))
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_stub_stop_agent_unsupported() {
        let adapter = LoongAdapter::new();
        let err = adapter.stop_agent(&LsId::new()).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_stub_list_agents_empty() {
        let adapter = LoongAdapter::new();
        let agents = adapter.list_agents().await;
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_stub_runtime_status() {
        let adapter = LoongAdapter::new();
        let status = adapter.runtime_status().await.unwrap();
        assert_eq!(status["agent_count"], 0);
        assert_eq!(status["runtime_health"], "stub");
    }

    #[tokio::test]
    async fn test_stub_with_registry() {
        let registry = Arc::new(AgentRegistry::new());
        let _adapter = LoongAdapter::new().with_registry(registry.clone());
        // Stub should not panic and registry should remain empty
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn test_stub_new_and_default() {
        let adapter = LoongAdapter::new();
        let _default: LoongAdapter = Default::default();
        let agents = adapter.list_agents().await;
        assert!(agents.is_empty());
    }
}
