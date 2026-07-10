//! 凭证管理 MCP 工具 — 供 AI Agent 通过 MCP 协议管理多 Git 提供商凭证
//!
//! 注册这些工具后，AI 可以通过 `tools/call` 创建、查询、删除和验证凭证。
//!
//! ## 可用工具
//!
//! | 工具名 | 功能 | 所需参数 |
//! |--------|------|----------|
//! | `credential_list` | 列出所有凭证 | `provider` (可选) |
//! | `credential_create` | 创建新凭证 | `provider`, `credential_type`, `name`, `token` |
//! | `credential_get` | 查询凭证摘要 | `id` |
//! | `credential_delete` | 删除凭证 | `id` |
//! | `credential_validate` | 验证凭证有效性 | `id` |

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_credentials::CredentialManager;
use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};
use serde_json::Value;
use std::sync::Arc;

// ── 辅助函数 ────────────────────────────────────────

/// 从 Value 中提取字符串字段
macro_rules! str_field {
    ($v:expr, $name:expr) => {
        $v.get($name)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
}

/// 构造工具失败的 MCP 响应
fn error_result(msg: String) -> Value {
    serde_json::json!({
        "is_error": true,
        "content": [{"type": "text", "text": msg}]
    })
}

/// 构造工具成功的 MCP 响应
fn success_result(data: Value) -> Value {
    serde_json::json!({
        "is_error": false,
        "content": [{"type": "text", "text": serde_json::to_string_pretty(&data).unwrap_or_default()}]
    })
}

// ── CredentialListTool ──────────────────────────────

/// 列出凭证 — 可选按提供商过滤
pub struct CredentialListTool {
    mgr: Arc<CredentialManager>,
}

impl CredentialListTool {
    pub fn new(mgr: Arc<CredentialManager>) -> Self {
        Self { mgr }
    }
}

#[async_trait]
impl Tool for CredentialListTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "credential_list".into(),
            description: "列出所有已存储的 Git 平台凭证，可按 provider 过滤。provider 可选值: gitee, codeup, coding, gitcode, cnb".into(),
            parameters: vec![ToolParam {
                name: "provider".into(),
                description: "可选，按提供商过滤。不传则列出全部。".into(),
                required: false,
                param_type: "string".into(),
            }],
        ..Default::default()
        }
    }

    fn validate(&self, _input: &Value) -> LsResult<()> {
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let list = if let Some(provider) = str_field!(input, "provider") {
            self.mgr
                .list_by_provider(&provider)
                .map_err(|e| LsError::Internal(format!("list credentials by provider: {e}")))?
        } else {
            self.mgr
                .list()
                .map_err(|e| LsError::Internal(format!("list credentials: {e}")))?
        };

        Ok(success_result(
            serde_json::to_value(list).unwrap_or_default(),
        ))
    }
    fn duplicate(&self) -> Box<dyn Tool> { Box::new(CredentialListTool { mgr: self.mgr.clone() }) }
}

// ── CredentialCreateTool ────────────────────────────

/// 创建凭证
pub struct CredentialCreateTool {
    mgr: Arc<CredentialManager>,
}

impl CredentialCreateTool {
    pub fn new(mgr: Arc<CredentialManager>) -> Self {
        Self { mgr }
    }
}

#[async_trait]
impl Tool for CredentialCreateTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "credential_create".into(),
            description: "创建一条新的 Git 平台凭证，加密存储。provider 可选值: gitee, codeup, coding, gitcode, cnb。credential_type 可选值: personal_access_token, enterprise_token, deployment_token, access_token。默认 skip_validation=true 跳过 API 验证直接存储。".into(),
            parameters: vec![
                ToolParam {
                    name: "provider".into(),
                    description: "Git 提供商名称".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "credential_type".into(),
                    description: "凭证类型".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "name".into(),
                    description: "凭证名称（便于识别）".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "token".into(),
                    description: "API 访问令牌".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "description".into(),
                    description: "凭证描述".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "username".into(),
                    description: "用户名（部分平台需要）".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "base_url".into(),
                    description: "自定义 API 基础 URL".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "scopes".into(),
                    description: "权限范围列表，逗号分隔".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "skip_validation".into(),
                    description: "是否跳过 API 验证直接存储，默认 true".into(),
                    required: false,
                    param_type: "boolean".into(),
                },
            ],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if !input.is_object() {
            return Err(LsError::Validation("input must be a JSON object".into()));
        }
        if str_field!(input, "provider").is_none() {
            return Err(LsError::Validation("provider is required".into()));
        }
        if str_field!(input, "credential_type").is_none() {
            return Err(LsError::Validation("credential_type is required".into()));
        }
        if str_field!(input, "name").is_none() {
            return Err(LsError::Validation("name is required".into()));
        }
        if str_field!(input, "token").is_none() {
            return Err(LsError::Validation("token is required".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let scopes: Vec<String> = input
            .get("scopes")
            .and_then(|v| v.as_str())
            .map(|s| {
                s.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let skip_validation = input
            .get("skip_validation")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let req = lingshu_credentials::CreateCredentialRequest {
            provider: str_field!(input, "provider").unwrap(),
            credential_type: str_field!(input, "credential_type").unwrap(),
            name: str_field!(input, "name").unwrap(),
            description: str_field!(input, "description"),
            token: str_field!(input, "token").unwrap(),
            username: str_field!(input, "username"),
            base_url: str_field!(input, "base_url"),
            scopes: Some(scopes),
            permissions_group: None,
            expires_at: None,
        };

        let summary = if skip_validation {
            self.mgr
                .create_without_validate(req)
                .map_err(|e| LsError::Internal(format!("create credential: {e}")))?
        } else {
            self.mgr
                .create(req)
                .await
                .map_err(|e| LsError::Internal(format!("create credential: {e}")))?
        };

        Ok(success_result(
            serde_json::to_value(summary).unwrap_or_default(),
        ))
    }
    fn duplicate(&self) -> Box<dyn Tool> { Box::new(CredentialCreateTool { mgr: self.mgr.clone() }) }
}

// ── CredentialGetTool ───────────────────────────────

/// 查询凭证摘要
pub struct CredentialGetTool {
    mgr: Arc<CredentialManager>,
}

impl CredentialGetTool {
    pub fn new(mgr: Arc<CredentialManager>) -> Self {
        Self { mgr }
    }
}

#[async_trait]
impl Tool for CredentialGetTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "credential_get".into(),
            description: "按 ID 查询已存储的 Git 平台凭证摘要（不暴露 token 原文）".into(),
            parameters: vec![ToolParam {
                name: "id".into(),
                description: "凭证 ID".into(),
                required: true,
                param_type: "string".into(),
            }],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if str_field!(input, "id").is_none() {
            return Err(LsError::Validation("id is required".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let id = str_field!(input, "id").unwrap();
        match self.mgr.get_summary(&id) {
            Ok(summary) => Ok(success_result(
                serde_json::to_value(summary).unwrap_or_default(),
            )),
            Err(e) => Ok(error_result(format!("credential not found: {e}"))),
        }
    }
    fn duplicate(&self) -> Box<dyn Tool> { Box::new(CredentialGetTool { mgr: self.mgr.clone() }) }
}

// ── CredentialDeleteTool ────────────────────────────

/// 删除凭证
pub struct CredentialDeleteTool {
    mgr: Arc<CredentialManager>,
}

impl CredentialDeleteTool {
    pub fn new(mgr: Arc<CredentialManager>) -> Self {
        Self { mgr }
    }
}

#[async_trait]
impl Tool for CredentialDeleteTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "credential_delete".into(),
            description: "删除指定 ID 的 Git 平台凭证".into(),
            parameters: vec![ToolParam {
                name: "id".into(),
                description: "要删除的凭证 ID".into(),
                required: true,
                param_type: "string".into(),
            }],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if str_field!(input, "id").is_none() {
            return Err(LsError::Validation("id is required".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let id = str_field!(input, "id").unwrap();
        match self.mgr.delete(&id) {
            Ok(true) => Ok(success_result(
                serde_json::json!({"deleted": true, "id": id}),
            )),
            Ok(false) => Ok(error_result(format!("credential not found: {id}"))),
            Err(e) => Ok(error_result(format!("delete failed: {e}"))),
        }
    }
    fn duplicate(&self) -> Box<dyn Tool> { Box::new(CredentialDeleteTool { mgr: self.mgr.clone() }) }
}

// ── CredentialValidateTool ──────────────────────────

/// 验证凭证
pub struct CredentialValidateTool {
    mgr: Arc<CredentialManager>,
}

impl CredentialValidateTool {
    pub fn new(mgr: Arc<CredentialManager>) -> Self {
        Self { mgr }
    }
}

#[async_trait]
impl Tool for CredentialValidateTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "credential_validate".into(),
            description: "验证指定 ID 的凭证是否有效，通过对目标 Git 提供商 API 做一次实际调用"
                .into(),
            parameters: vec![ToolParam {
                name: "id".into(),
                description: "要验证的凭证 ID".into(),
                required: true,
                param_type: "string".into(),
            }],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if str_field!(input, "id").is_none() {
            return Err(LsError::Validation("id is required".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let id = str_field!(input, "id").unwrap();
        match self.mgr.validate(&id).await {
            Ok(validation) => Ok(success_result(
                serde_json::to_value(validation).unwrap_or_default(),
            )),
            Err(e) => Ok(error_result(format!("validate failed: {e}"))),
        }
    }
    fn duplicate(&self) -> Box<dyn Tool> { Box::new(CredentialValidateTool { mgr: self.mgr.clone() }) }
}

// ── CredentialUpdateTool ────────────────────────────

/// 更新凭证
pub struct CredentialUpdateTool {
    mgr: Arc<CredentialManager>,
}

impl CredentialUpdateTool {
    pub fn new(mgr: Arc<CredentialManager>) -> Self {
        Self { mgr }
    }
}

#[async_trait]
impl Tool for CredentialUpdateTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "credential_update".into(),
            description: "更新指定 ID 的凭证字段。只传需要修改的字段，未传的字段保持不变。不能修改 provider 和 credential_type。".into(),
            parameters: vec![
                ToolParam {
                    name: "id".into(),
                    description: "要更新的凭证 ID".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "name".into(),
                    description: "新名称".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "description".into(),
                    description: "新描述".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "token".into(),
                    description: "新令牌".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "username".into(),
                    description: "用户名".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "base_url".into(),
                    description: "自定义 API 基础 URL".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "scopes".into(),
                    description: "权限范围列表，逗号分隔".into(),
                    required: false,
                    param_type: "string".into(),
                },
            ],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if !input.is_object() {
            return Err(LsError::Validation("input must be a JSON object".into()));
        }
        if str_field!(input, "id").is_none() {
            return Err(LsError::Validation("id is required".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let id = str_field!(input, "id").unwrap();
        let scopes: Option<Vec<String>> = input.get("scopes").and_then(|v| v.as_str()).map(|s| {
            s.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        });

        let req = lingshu_credentials::UpdateCredentialRequest {
            name: str_field!(input, "name"),
            description: str_field!(input, "description"),
            token: str_field!(input, "token"),
            username: str_field!(input, "username"),
            base_url: str_field!(input, "base_url"),
            scopes,
            permissions_group: None,
            expires_at: None,
        };

        match self.mgr.update(&id, req).await {
            Ok(true) => Ok(success_result(
                serde_json::json!({"updated": true, "id": id}),
            )),
            Ok(false) => Ok(error_result(format!("credential not found: {id}"))),
            Err(e) => Ok(error_result(format!("update failed: {e}"))),
        }
    }
    fn duplicate(&self) -> Box<dyn Tool> { Box::new(CredentialUpdateTool { mgr: self.mgr.clone() }) }
}

// ── 批量注册辅助 ────────────────────────────────────

/// 创建一组凭证管理 MCP 工具
pub fn create_credential_tools(mgr: Arc<CredentialManager>) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(CredentialListTool::new(mgr.clone())),
        Arc::new(CredentialCreateTool::new(mgr.clone())),
        Arc::new(CredentialGetTool::new(mgr.clone())),
        Arc::new(CredentialUpdateTool::new(mgr.clone())),
        Arc::new(CredentialDeleteTool::new(mgr.clone())),
        Arc::new(CredentialValidateTool::new(mgr)),
    ]
}
