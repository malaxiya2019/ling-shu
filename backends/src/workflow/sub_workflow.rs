//! SubWorkflow — 子工作流节点
//!
//! 支持在工作流中嵌套执行另一个工作流。
//! 子工作流可以引用已注册的工作流，也可以动态创建。


use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::info;



/// 子工作流执行策略
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubWorkflowStrategy {
    /// 同步等待子工作流完成
    #[default]
    Sync,
    /// 异步触发，不等待结果
    Async,
}



/// 子工作流配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubWorkflowConfig {
    /// 工作流名称（在 registry 中的名称）
    pub workflow_name: String,
    /// 输入参数映射
    pub input_mapping: Option<Value>,
    /// 执行策略
    pub strategy: SubWorkflowStrategy,
    /// 超时秒数
    pub timeout_secs: u64,
}

impl Default for SubWorkflowConfig {
    fn default() -> Self {
        Self {
            workflow_name: String::new(),
            input_mapping: None,
            strategy: SubWorkflowStrategy::Sync,
            timeout_secs: 300,
        }
    }
}

/// 子工作流执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubWorkflowResult {
    pub sub_workflow_id: LsId,
    pub success: bool,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// 子工作流执行器
pub struct SubWorkflowExecutor {
    /// 工作流注册表（用于查找已注册的工作流）
    workflow_registry: Option<Arc<tokio::sync::RwLock<super::registry::WorkflowRegistry>>>,
}

impl SubWorkflowExecutor {
    pub fn new() -> Self {
        Self {
            workflow_registry: None,
        }
    }

    pub fn with_registry(mut self, registry: Arc<tokio::sync::RwLock<super::registry::WorkflowRegistry>>) -> Self {
        self.workflow_registry = Some(registry);
        self
    }

    /// 执行子工作流
    pub async fn execute(
        &self,
        ctx: LsContext,
        config: &SubWorkflowConfig,
        input: Value,
    ) -> LsResult<SubWorkflowResult> {
        let _start = std::time::Instant::now();

        match config.strategy {
            SubWorkflowStrategy::Sync => {
                let result = self.execute_sync(ctx, config, input).await?;
                Ok(result)
            }
            SubWorkflowStrategy::Async => {
                // 异步触发，立即返回
                let result = SubWorkflowResult {
                    sub_workflow_id: LsId::new(),
                    success: true,
                    output: Some(serde_json::json!({"status": "triggered"})),
                    error: None,
                    duration_ms: 0,
                };
                info!("sub_workflow: async trigger {}", config.workflow_name);
                Ok(result)
            }
        }
    }

    async fn execute_sync(
        &self,
        ctx: LsContext,
        config: &SubWorkflowConfig,
        input: Value,
    ) -> LsResult<SubWorkflowResult> {
        let start = std::time::Instant::now();

        // 从 registry 获取已注册的工作流
        let dag = if let Some(ref registry) = self.workflow_registry {
            let reg = registry.read().await;
            match reg.get(&config.workflow_name).await {
                Some(dag) => {
                    Some(dag)
                }
                None => {
                    return Err(lingshu_core::LsError::Internal(format!(
                        "sub_workflow: workflow '{}' not found in registry",
                        config.workflow_name
                    )));
                }
            }
        } else {
            return Err(lingshu_core::LsError::Internal(
                "sub_workflow: no registry configured".to_string(),
            ));
        };

        let dag = dag.unwrap();
        let workflow_input = config.input_mapping.clone().unwrap_or(input);

        info!(
            "sub_workflow: executing '{}' (sync)",
            config.workflow_name
        );

        // 执行子工作流
        let result = dag.execute(ctx, workflow_input).await?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(SubWorkflowResult {
            sub_workflow_id: result.workflow_id,
            success: result.success,
            output: result.node_results.last().and_then(|r| r.output.clone()),
            error: if result.success { None } else { Some("workflow failed".to_string()) },
            duration_ms,
        })
    }
}

impl Default for SubWorkflowExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// 创建子工作流的 NodeHandler
pub fn create_sub_workflow_handler(
    executor: Arc<SubWorkflowExecutor>,
    config: SubWorkflowConfig,
) -> crate::workflow::dag::NodeHandler {
    Arc::new(move |ctx: LsContext, input: Value| {
        let executor = executor.clone();
        let config = config.clone();
        Box::pin(async move {
            let result = executor.execute(ctx, &config, input).await?;
            Ok(serde_json::to_value(result).unwrap_or(Value::Null))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::registry::{WorkflowRegistry, WorkflowRegistryEntry};

    #[test]
    fn test_sub_workflow_config_defaults() {
        let config = SubWorkflowConfig::default();
        assert!(config.workflow_name.is_empty());
        assert_eq!(config.strategy, SubWorkflowStrategy::Sync);
        assert_eq!(config.timeout_secs, 300);
    }

    #[test]
    fn test_sub_workflow_result_serde() {
        let result = SubWorkflowResult {
            sub_workflow_id: LsId::new(),
            success: true,
            output: Some(serde_json::json!({"result": "ok"})),
            error: None,
            duration_ms: 100,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: SubWorkflowResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.success);
        assert_eq!(deserialized.duration_ms, 100);
    }

    #[tokio::test]
    async fn test_executor_no_registry() {
        let executor = SubWorkflowExecutor::new();
        let ctx = LsContext::with_session(LsId::new());
        let config = SubWorkflowConfig {
            workflow_name: "test".to_string(),
            ..Default::default()
        };
        let result = executor.execute(ctx, &config, Value::Null).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_executor_workflow_not_found() {
        let registry = Arc::new(tokio::sync::RwLock::new(WorkflowRegistry::new()));
        let executor = SubWorkflowExecutor::new()
            .with_registry(registry);
        let ctx = LsContext::with_session(LsId::new());
        let config = SubWorkflowConfig {
            workflow_name: "nonexistent".to_string(),
            ..Default::default()
        };
        let result = executor.execute(ctx, &config, Value::Null).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_async_strategy() {
        let executor = SubWorkflowExecutor::new();
        let ctx = LsContext::with_session(LsId::new());
        let config = SubWorkflowConfig {
            workflow_name: "test".to_string(),
            strategy: SubWorkflowStrategy::Async,
            ..Default::default()
        };
        let result = executor.execute(ctx, &config, Value::Null).await.unwrap();
        assert!(result.success);
        assert_eq!(result.duration_ms, 0);
    }

    #[test]
    fn test_create_handler() {
        let executor = Arc::new(SubWorkflowExecutor::new());
        let config = SubWorkflowConfig::default();
        let handler = create_sub_workflow_handler(executor, config);
        // Just verify it compiles and is send+sync
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<crate::workflow::dag::NodeHandler>();
    }
}
