//! ToolRegistry v2 — 增强版工具注册与执行管理中心.

use std::collections::HashMap;
use std::sync::Arc;

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::llm::ToolDefinition;
use lingshu_traits::tool::{Tool, ToolCallRecord, ToolInfo};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::info;

use crate::permission::{CallerInfo, ToolPermission};
use crate::sandbox::ToolSandbox;
use crate::types::{ToolExecutionResult, ToolFilter, ToolStats};

/// 增强版工具注册表.
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, Box<dyn Tool>>>>,
    tag_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    category_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    history: Arc<RwLock<Vec<ToolCallRecord>>>,
    permission: Arc<RwLock<ToolPermission>>,
    sandbox: ToolSandbox,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            tag_index: Arc::new(RwLock::new(HashMap::new())),
            category_index: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            permission: Arc::new(RwLock::new(ToolPermission::new())),
            sandbox: ToolSandbox::new(),
        }
    }

    pub fn with_sandbox(mut self, sandbox: ToolSandbox) -> Self {
        self.sandbox = sandbox;
        self
    }

    // ── 注册 / 注销 ──

    /// 注册工具，自动更新标签和分类索引.
    pub async fn register(&self, tool: Box<dyn Tool>) {
        let info = tool.info();
        let name = info.name.clone();

        self.tools.write().await.insert(name.clone(), tool);

        // 标签索引
        if !info.metadata.tags.is_empty() {
            let mut tag_idx = self.tag_index.write().await;
            for tag in &info.metadata.tags {
                tag_idx.entry(tag.clone()).or_default().push(name.clone());
            }
        }

        // 分类索引
        let cat = info.metadata.category.as_str().to_string();
        self.category_index.write().await
            .entry(cat).or_default().push(name.clone());

        info!(tool = %name, category = %info.metadata.category, "tool registered");
    }

    /// 批量注册.
    pub async fn register_all(&self, tools: Vec<Box<dyn Tool>>) {
        for tool in tools {
            self.register(tool).await;
        }
    }

    /// 注销工具.
    pub async fn unregister(&self, name: &str) -> bool {
        let removed = self.tools.write().await.remove(name).is_some();
        if removed {
            self.tag_index.write().await.retain(|_, names| {
                names.retain(|n| n != name);
                !names.is_empty()
            });
            self.category_index.write().await.retain(|_, names| {
                names.retain(|n| n != name);
                !names.is_empty()
            });
            info!(tool = %name, "tool unregistered");
        }
        removed
    }

    // ── 查找 / 过滤 ──

    pub async fn get(&self, name: &str) -> Option<ToolInfo> {
        self.tools.read().await.get(name).map(|t| t.info())
    }

    pub async fn list_tools(&self) -> Vec<String> {
        self.tools.read().await.keys().cloned().collect()
    }

    pub async fn list_by_category(&self, category: &str) -> Vec<String> {
        self.category_index.read().await
            .get(category).cloned().unwrap_or_default()
    }

    pub async fn list_by_tag(&self, tag: &str) -> Vec<String> {
        self.tag_index.read().await
            .get(tag).cloned().unwrap_or_default()
    }

    pub async fn find(&self, filter: &ToolFilter) -> Vec<ToolInfo> {
        let tools = self.tools.read().await;
        let mut results = Vec::new();
        for (name, tool) in tools.iter() {
            let info = tool.info();
            if let Some(ref cat) = filter.category {
                if info.metadata.category.as_str() != cat.as_str() { continue; }
            }
            if let Some(ref tags) = filter.tags {
                if !tags.iter().any(|t| info.metadata.tags.contains(t)) { continue; }
            }
            if let Some(ref search) = filter.name_search {
                if !name.to_lowercase().contains(&search.to_lowercase()) { continue; }
            }
            results.push(info);
        }
        results
    }

    // ── 执行 ──

    /// 执行工具（带权限检查和沙箱封装）.
    pub async fn execute(
        &self,
        ctx: &LsContext,
        name: &str,
        args: Value,
        caller: Option<&CallerInfo>,
    ) -> LsResult<Value> {
        // 在锁内获取 tool_info，然后将引用传出
        let tool_info;
        {
            let tools = self.tools.read().await;
            let t = tools.get(name)
                .ok_or_else(|| LsError::NotFound(format!("tool not found: {name}")))?;
            tool_info = t.info();
        }

        // 权限检查
        if let Some(caller) = caller {
            let perm = self.permission.read().await;
            perm.check(name, &tool_info.metadata.permission_level, caller)?;
        }

        // 再次获取锁获取 tool 引用执行
        let (output, duration_ms) = {
            // 重新获取锁安全地获取引用
            let tools = self.tools.read().await;
            let tool = tools.get(name)
                .ok_or_else(|| LsError::NotFound(format!("tool not found: {name}")))?;
            self.sandbox.execute(tool.as_ref(), ctx.clone(), args.clone()).await?
        };

        // 调用历史
        let record = ToolCallRecord {
            tool_id: tool_info.tool_id,
            call_id: LsId::new(),
            session_id: ctx.session_id,
            input: args,
            output: output.clone(),
            duration_ms,
            success: true,
            timestamp: chrono::Utc::now(),
            caller: caller.and_then(|c| c.user_id.clone()),
            error: None,
        };
        self.history.write().await.push(record);

        Ok(output)
    }

    /// 简化执行（无权限检查，向后兼容）.
    pub async fn execute_unchecked(
        &self,
        ctx: &LsContext,
        name: &str,
        args: Value,
    ) -> LsResult<Value> {
        self.execute(ctx, name, args, None).await
    }

    // ── 权限管理 ──

    pub async fn permission(&self) -> tokio::sync::RwLockReadGuard<'_, ToolPermission> {
        self.permission.read().await
    }

    pub async fn permission_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, ToolPermission> {
        self.permission.write().await
    }

    // ── OpenAI 兼容接口 ──

    pub async fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        let tools = self.tools.read().await;
        let mut defs = Vec::with_capacity(tools.len());
        for tool in tools.values() {
            let info = tool.info();
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for param in &info.parameters {
                let mut prop = serde_json::Map::new();
                prop.insert("type".into(), serde_json::json!(param.param_type));
                prop.insert("description".into(), serde_json::json!(param.description));
                properties.insert(param.name.clone(), serde_json::Value::Object(prop));
                if param.required {
                    required.push(param.name.clone());
                }
            }
            defs.push(ToolDefinition {
                tool_type: "function".into(),
                function: lingshu_traits::llm::ToolFunction {
                    name: info.name.clone(),
                    description: info.description.clone(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": properties,
                        "required": required,
                    }),
                },
            });
        }
        defs
    }

    // ── 统计 / 历史 ──

    pub async fn history(&self) -> Vec<ToolCallRecord> {
        self.history.read().await.clone()
    }

    pub async fn recent_history(&self, n: usize) -> Vec<ToolCallRecord> {
        let h = self.history.read().await;
        let len = h.len();
        h.iter().skip(len.saturating_sub(n)).cloned().collect()
    }

    pub async fn count(&self) -> usize {
        self.tools.read().await.len()
    }

    pub async fn stats(&self) -> ToolStats {
        // 分类统计
        let by_category = {
            let tools = self.tools.read().await;
            let mut map: HashMap<String, usize> = HashMap::new();
            for tool in tools.values() {
                let cat = tool.info().metadata.category.as_str().to_string();
                *map.entry(cat).or_insert(0) += 1;
            }
            map
        };

        let total_count = self.tools.read().await.len();
        let history = self.history.read().await;
        let total_calls = history.len() as u64;
        let recent_calls: Vec<ToolExecutionResult> = history
            .iter()
            .rev()
            .take(10)
            .map(|r| ToolExecutionResult {
                call_id: r.call_id.to_string(),
                tool_name: r.tool_id.to_string(),
                success: r.success,
                output: r.output.clone(),
                duration_ms: r.duration_ms,
                error: r.error.clone(),
                timestamp: r.timestamp,
            })
            .collect();

        ToolStats {
            total_count,
            by_category,
            total_calls,
            recent_calls,
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_core::LsId;
    use lingshu_traits::tool::{ToolCategory, ToolMetadata, PermissionLevel, ToolParam};

    struct EchoTool;
    #[async_trait]
    impl Tool for EchoTool {
        fn info(&self) -> ToolInfo {
            ToolInfo {
                tool_id: LsId::new(),
                name: "echo".into(),
                description: "Echo the input".into(),
                parameters: vec![ToolParam {
                    name: "message".into(), description: "Message to echo".into(),
                    required: true, param_type: "string".into(),
                }],
                metadata: ToolMetadata {
                    category: ToolCategory::General,
                    tags: vec!["utility".into(), "test".into()],
                    permission_level: PermissionLevel::Public,
                    timeout_ms: None,
                    sandbox_config: None,
                    version: "1.0.0".into(),
                    author: "lingshu".into(),
                },
            }
        }
        fn validate(&self, input: &Value) -> LsResult<()> {
            if input.get("message").and_then(|v| v.as_str()).is_none() {
                return Err(LsError::Validation("missing message".into()));
            }
            Ok(())
        }
        async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> { Ok(input) }
    
    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(EchoTool)
    }
}

    struct FileTool;
    #[async_trait]
    impl Tool for FileTool {
        fn info(&self) -> ToolInfo {
            ToolInfo {
                tool_id: LsId::new(),
                name: "read_file".into(), description: "Read a file".into(),
                parameters: vec![],
                metadata: ToolMetadata {
                    category: ToolCategory::FileSystem,
                    tags: vec!["filesystem".into()],
                    permission_level: PermissionLevel::User,
                    timeout_ms: None,
                    sandbox_config: None,
                    version: "1.0.0".into(),
                    author: "lingshu".into(),
                },
            }
        }
        fn duplicate(&self) -> Box<dyn Tool> { Box::new(FileTool) }
        fn validate(&self, _input: &Value) -> LsResult<()> { Ok(()) }
        async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> { Ok(input) }
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let reg = ToolRegistry::new();
        reg.register(Box::new(EchoTool)).await;
        reg.register(Box::new(FileTool)).await;
        assert_eq!(reg.count().await, 2);
        assert!(reg.list_tools().await.contains(&"echo".to_string()));
    }

    #[tokio::test]
    async fn test_list_by_category() {
        let reg = ToolRegistry::new();
        reg.register(Box::new(EchoTool)).await;
        reg.register(Box::new(FileTool)).await;
        assert!(reg.list_by_category("general").await.contains(&"echo".to_string()));
        assert!(reg.list_by_category("filesystem").await.contains(&"read_file".to_string()));
    }

    #[tokio::test]
    async fn test_list_by_tag() {
        let reg = ToolRegistry::new();
        reg.register(Box::new(EchoTool)).await;
        assert!(reg.list_by_tag("utility").await.contains(&"echo".to_string()));
    }

    #[tokio::test]
    async fn test_unregister() {
        let reg = ToolRegistry::new();
        reg.register(Box::new(EchoTool)).await;
        assert!(reg.unregister("echo").await);
        assert_eq!(reg.count().await, 0);
    }

    #[tokio::test]
    async fn test_execute() {
        let reg = ToolRegistry::new();
        reg.register(Box::new(EchoTool)).await;
        let ctx = LsContext::with_session(LsId::new());
        let result = reg.execute_unchecked(&ctx, "echo", serde_json::json!({"message":"hi"})).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["message"], "hi");
    }

    #[tokio::test]
    async fn test_execute_with_permission() {
        let reg = ToolRegistry::new();
        reg.register(Box::new(FileTool)).await; // requires User
        let ctx = LsContext::with_session(LsId::new());
        assert!(reg.execute(&ctx, "read_file", serde_json::json!({}), Some(&CallerInfo::anonymous())).await.is_err());
        assert!(reg.execute(&ctx, "read_file", serde_json::json!({}), Some(&CallerInfo::user("u1"))).await.is_ok());
    }

    #[tokio::test]
    async fn test_tool_definitions() {
        let reg = ToolRegistry::new();
        reg.register(Box::new(EchoTool)).await;
        let defs = reg.get_tool_definitions().await;
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "echo");
    }
}
