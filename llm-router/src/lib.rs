//! LLRouter — Multi-LLM Router
//!
//! 动态多 LLM 提供商路由选择，支持以下策略：
//! - **Priority**: 按优先级顺序选择可用后端
//! - **Fallback**: 主后端失败时自动降级
//! - **Latency**: 选择历史延迟最低的后端
//! - **Cost**: 选择成本最低的后端
//! - **RoundRobin**: 轮询分发
//!
//! ## 架构
//!
//! ```text
//!  Client
//!    │
//!    ▼
//!  LlmRouter ──► RouterStrategy ──► BackendPool ──► Llm (provider)
//!    │                                    │
//!    │                                    ├── OpenAI
//!    │                HealthChecker      ├── Anthropic
//!    │                    │              ├── Groq
//!    │                    ▼              ├── LlamaCpp
//!    │               BackendHealth       ├── Mock
//!    │                                   └── ...
//!    └── MetricsCollector ──► BackendMetrics (latency, cost, error_rate)
//! ```

mod metrics;
mod strategies;

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::{Llm, LlmChunk, LlmRequest, LlmResponse};
use metrics::MetricsCollector;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use strategies::RouterStrategy;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ── Public exports ─────────────────────────────────

pub use metrics::BackendMetrics;
pub use strategies::{
    CostStrategy, FallbackStrategy, LatencyStrategy, PriorityStrategy, RoundRobinStrategy,
};

/// 路由策略类型.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[derive(Default)]
pub enum RouterPolicy {
    /// 按优先级顺序（默认）
    #[default]
    Priority,
    /// 主后端失败时降级
    Fallback,
    /// 选择历史延迟最低的后端
    Latency,
    /// 选择成本最低的后端
    Cost,
    /// 轮询分发
    RoundRobin,
}


/// 后端注册信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendEntry {
    /// 后端名称（如 "openai", "anthropic"）
    pub name: String,
    /// 显示名称
    pub label: String,
    /// 优先级（数字越小优先级越高）
    pub priority: u32,
    /// 每次请求成本（美元，用于 cost 策略）
    pub cost_per_request: f64,
    /// 是否启用
    pub enabled: bool,
    /// 支持的模型列表
    pub models: Vec<String>,
    /// 额外元数据
    pub metadata: HashMap<String, String>,
}

impl BackendEntry {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            label: String::new(),
            priority: 100,
            cost_per_request: 0.0,
            enabled: true,
            models: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_priority(mut self, p: u32) -> Self {
        self.priority = p;
        self
    }

    pub fn with_cost(mut self, cost: f64) -> Self {
        self.cost_per_request = cost;
        self
    }

    pub fn with_models(mut self, models: Vec<&str>) -> Self {
        self.models = models.into_iter().map(String::from).collect();
        self
    }
}

/// Multi-LLM Router — 动态多提供商路由.
#[allow(dead_code)]
pub struct LlmRouter {
    /// 已注册的后端（名称 → Llm）
    backends: HashMap<String, Box<dyn Llm + Send + Sync>>,
    /// 后端条目信息
    entries: HashMap<String, BackendEntry>,
    /// 当前路由策略
    strategy: Arc<RwLock<Box<dyn RouterStrategy + Send + Sync>>>,
    /// 指标收集器
    metrics: Arc<RwLock<MetricsCollector>>,
    /// 默认路由策略
    policy: RouterPolicy,
}

impl LlmRouter {
    /// 创建新的路由器.
    pub fn new(policy: RouterPolicy) -> Self {
        let strategy: Box<dyn RouterStrategy + Send + Sync> = match &policy {
            RouterPolicy::Priority => Box::new(PriorityStrategy::new()),
            RouterPolicy::Fallback => Box::new(FallbackStrategy::new()),
            RouterPolicy::Latency => Box::new(LatencyStrategy::new()),
            RouterPolicy::Cost => Box::new(CostStrategy::new()),
            RouterPolicy::RoundRobin => Box::new(RoundRobinStrategy::new()),
        };

        Self {
            backends: HashMap::new(),
            entries: HashMap::new(),
            strategy: Arc::new(RwLock::new(strategy)),
            metrics: Arc::new(RwLock::new(MetricsCollector::new())),
            policy,
        }
    }

    /// 注册一个 LLM 后端.
    pub fn register(
        &mut self,
        entry: BackendEntry,
        llm: Box<dyn Llm + Send + Sync>,
    ) {
        let name = entry.name.clone();
        info!(backend = %name, "registering LLM backend");
        self.backends.insert(name.clone(), llm);
        self.entries.insert(name, entry);
    }

    /// 获取当前所有注册的后端名称.
    pub fn backend_names(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, e)| e.enabled)
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// 获取后端指标.
    pub async fn get_metrics(&self, backend: &str) -> Option<BackendMetrics> {
        self.metrics.read().await.get(backend)
    }

    /// 获取所有后端指标.
    pub async fn all_metrics(&self) -> HashMap<String, BackendMetrics> {
        self.metrics.read().await.all()
    }

    /// 路由并调用 LLM.
    pub async fn route_and_invoke(
        &self,
        ctx: &LsContext,
        request: &LlmRequest,
    ) -> LsResult<LlmResponse> {
        let start = std::time::Instant::now();

        // 获取启用的后端列表
        let enabled: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.enabled)
            .map(|(n, _)| n.clone())
            .collect();

        if enabled.is_empty() {
            return Err(LsError::NotImplemented(
                "no LLM backends registered in router".into(),
            ));
        }

        // 使用策略选择后端
        let strategy = self.strategy.read().await;
        let selected = strategy.select(&enabled, &*self.metrics.read().await).await;

        let backend_name = match selected {
            Some(name) => name,
            None => {
                // 默认选择第一个
                enabled[0].clone()
            }
        };

        let llm = self.backends.get(&backend_name).ok_or_else(|| {
            LsError::Internal(format!("backend '{}' not found in router", backend_name))
        })?;

        info!(
            backend = %backend_name,
            model = %request.model,
            "routing LLM request"
        );

        // 调用
        let result = llm.invoke(ctx.clone(), request.clone()).await;

        // 记录指标
        let elapsed = start.elapsed();
        let mut metrics = self.metrics.write().await;
        metrics.record(
            &backend_name,
            elapsed,
            0.0, // cost tracked separately
            result.is_ok(),
        );

        // 如果失败且有 fallback，尝试降级
        if result.is_err() && enabled.len() > 1 {
            drop(metrics);
            let fallback = self.try_fallback(ctx, request, &enabled, &backend_name).await;
            if let Ok(resp) = fallback {
                let elapsed = start.elapsed();
                let mut metrics = self.metrics.write().await;
                metrics.record("fallback", elapsed, 0.0, true);
                return Ok(resp);
            }
        }

        result
    }

    /// 降级到下一个可用后端.
    async fn try_fallback(
        &self,
        ctx: &LsContext,
        request: &LlmRequest,
        enabled: &[String],
        failed: &str,
    ) -> LsResult<LlmResponse> {
        for name in enabled {
            if name == failed {
                continue;
            }
            if let Some(llm) = self.backends.get(name) {
                warn!(backend = %name, "fallback: trying alternative backend");
                match llm.invoke(ctx.clone(), request.clone()).await {
                    Ok(resp) => return Ok(resp),
                    Err(e) => {
                        warn!(backend = %name, error = %e, "fallback also failed");
                        continue;
                    }
                }
            }
        }
        Err(LsError::Internal("all backends exhausted in fallback".into()))
    }

    /// 流式路由并调用 LLM.
    pub async fn route_and_invoke_stream(
        &self,
        ctx: &LsContext,
        request: &LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        let enabled: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.enabled)
            .map(|(n, _)| n.clone())
            .collect();

        let backend_name = enabled.first().cloned().ok_or_else(|| {
            LsError::NotImplemented("no LLM backends registered".into())
        })?;

        let llm = self.backends.get(&backend_name).ok_or_else(|| {
            LsError::Internal(format!("backend '{}' not found", backend_name))
        })?;

        info!(
            backend = %backend_name,
            model = %request.model,
            "routing streaming LLM request"
        );

        llm.invoke_stream(ctx.clone(), request.clone()).await
    }

    /// 切换路由策略.
    pub async fn set_policy(&self, policy: RouterPolicy) {
        let new_strategy: Box<dyn RouterStrategy + Send + Sync> = match &policy {
            RouterPolicy::Priority => Box::new(PriorityStrategy::new()),
            RouterPolicy::Fallback => Box::new(FallbackStrategy::new()),
            RouterPolicy::Latency => Box::new(LatencyStrategy::new()),
            RouterPolicy::Cost => Box::new(CostStrategy::new()),
            RouterPolicy::RoundRobin => Box::new(RoundRobinStrategy::new()),
        };
        let mut strategy = self.strategy.write().await;
        *strategy = new_strategy;
        info!(?policy, "router policy changed");
    }

    /// 启用/禁用后端.
    pub async fn set_backend_enabled(&self, _name: &str, _enabled: bool) -> LsResult<()> {
        Err(LsError::NotImplemented("use register() to reconfigure".into()))
    }
}

/// 便捷构建器 — 从配置列表创建 LlmRouter.
pub struct RouterBuilder {
    backends: Vec<(BackendEntry, Box<dyn Llm + Send + Sync>)>,
    policy: RouterPolicy,
}

impl RouterBuilder {
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
            policy: RouterPolicy::default(),
        }
    }

    pub fn with_policy(mut self, policy: RouterPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn add_backend(
        mut self,
        entry: BackendEntry,
        llm: Box<dyn Llm + Send + Sync>,
    ) -> Self {
        self.backends.push((entry, llm));
        self
    }

    pub fn build(self) -> LlmRouter {
        let mut router = LlmRouter::new(self.policy);
        for (entry, llm) in self.backends {
            router.register(entry, llm);
        }
        router
    }
}

impl Default for RouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Llm for LlmRouter {
    async fn invoke(&self, ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        self.route_and_invoke(&ctx, &request).await
    }

    async fn invoke_stream(
        &self,
        ctx: LsContext,
        request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        self.route_and_invoke_stream(&ctx, &request).await
    }

    async fn usage_stats(&self, ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let mut total = HashMap::new();
        for (name, llm) in &self.backends {
            if let Ok(stats) = llm.usage_stats(ctx.clone()).await {
                for (k, v) in stats {
                    *total.entry(format!("{name}_{k}")).or_insert(0) += v;
                }
            }
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;
    use lingshu_traits::llm::{LlmMessage, LlmRole};

    fn mock_llm() -> Box<dyn Llm + Send + Sync> {
        Box::new(lingshu_backends::MockLlm::new())
    }

    #[tokio::test]
    async fn test_router_creation() {
        let router = LlmRouter::new(RouterPolicy::Priority);
        assert!(router.backend_names().is_empty());
    }

    #[tokio::test]
    async fn test_register_backend() {
        let mut router = LlmRouter::new(RouterPolicy::Priority);
        router.register(
            BackendEntry::new("mock").with_priority(1),
            mock_llm(),
        );
        let names = router.backend_names();
        assert_eq!(names, vec!["mock"]);
    }

    #[tokio::test]
    async fn test_route_and_invoke() {
        let mut router = LlmRouter::new(RouterPolicy::Priority);
        router.register(
            BackendEntry::new("mock").with_priority(1),
            mock_llm(),
        );

        let ctx = LsContext::with_session(LsId::new());
        let req = LlmRequest {
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: "hello".into(),
                content_parts: None,
                name: None,
                tool_calls: None,
            }],
            temperature: None,
            max_tokens: None,
            stream: false,
            model: "mock".into(),
            tools: None,
        };

        let resp = router.route_and_invoke(&ctx, &req).await.unwrap();
        assert!(!resp.message.content.is_empty());
    }

    #[tokio::test]
    async fn test_fallback_on_failure() {
        // Use a router with no real backends — should return fallback error
        let router = LlmRouter::new(RouterPolicy::Fallback);
        let ctx = LsContext::with_session(LsId::new());
        let req = LlmRequest {
            model: "mock".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            tools: None,
            stream: false,
        };
        let result = router.route_and_invoke(&ctx, &req).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let mut router = LlmRouter::new(RouterPolicy::Priority);
        router.register(
            BackendEntry::new("mock").with_priority(1),
            mock_llm(),
        );

        let ctx = LsContext::with_session(LsId::new());
        let req = LlmRequest {
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: "test".into(),
                content_parts: None,
                name: None,
                tool_calls: None,
            }],
            temperature: None,
            max_tokens: None,
            stream: false,
            model: "mock".into(),
            tools: None,
        };

        let _ = router.route_and_invoke(&ctx, &req).await;
        let metrics = router.get_metrics("mock").await;
        assert!(metrics.is_some());
        assert!(metrics.unwrap().total_calls >= 1);
    }

    #[tokio::test]
    async fn test_policy_switch() {
        let router = LlmRouter::new(RouterPolicy::Priority);
        router.set_policy(RouterPolicy::RoundRobin).await;
        // Verify we can still route after policy switch
        let ctx = LsContext::with_session(LsId::new());
        let req = LlmRequest {
            model: "mock".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            tools: None,
            stream: false,
        };
        let result = router.route_and_invoke(&ctx, &req).await;
        assert!(result.is_err()); // No backends
    }

    #[tokio::test]
    async fn test_llm_trait_invoke() {
        let mut router = LlmRouter::new(RouterPolicy::Priority);
        router.register(
            BackendEntry::new("mock").with_priority(1),
            mock_llm(),
        );

        let ctx = LsContext::with_session(LsId::new());
        let req = LlmRequest {
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: "hello from trait".into(),
                content_parts: None,
                name: None,
                tool_calls: None,
            }],
            model: "mock".into(),
            temperature: None,
            max_tokens: None,
            tools: None,
            stream: false,
        };

        let resp = Llm::invoke(&router, ctx, req).await.unwrap();
        assert!(!resp.message.content.is_empty());
    }
}
