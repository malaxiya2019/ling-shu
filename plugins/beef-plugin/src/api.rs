//! 🔌 BeEF REST API 整合 — 通过 BeEF HTTP API 控制 hooked browsers.
//!
//! 提供类型安全的 Rust 接口，包装 BeEF 的 REST 端点。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── BeEF 认证 ───────────────────────────────────────

/// BeEF 登录请求.
#[derive(Debug, Serialize)]
pub struct BeefLogin {
    pub username: String,
    pub password: String,
}

/// BeEF 登录响应.
#[derive(Debug, Deserialize)]
pub struct BeefLoginResponse {
    pub success: bool,
    pub token: Option<String>,
    #[serde(rename = "session_id")]
    pub session_id: Option<String>,
}

// ── Hooked Browsers ─────────────────────────────────

/// 被钩住的浏览器信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookedBrowser {
    pub id: String,
    pub ip: String,
    pub browser: String,
    pub version: String,
    pub os: String,
    pub platform: String,
    pub domain: String,
    pub hooked_at: String,
    pub status: String,
    pub session: Option<String>,
}

/// BeEF /api/hooks 响应.
#[derive(Debug, Deserialize)]
pub struct HooksResponse {
    pub success: bool,
    pub hooks: Option<HashMap<String, HashMap<String, serde_json::Value>>>,
    #[serde(rename = "online")]
    pub online_count: Option<u32>,
    #[serde(rename = "offline")]
    pub offline_count: Option<u32>,
}

// ── Modules ─────────────────────────────────────────

/// BeEF 模块信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeefModule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub author: String,
    pub targets: HashMap<String, String>,
}

/// 模块执行请求.
#[derive(Debug, Serialize)]
pub struct ModuleExecRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zombie: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    pub options: HashMap<String, serde_json::Value>,
}

/// 模块执行响应.
#[derive(Debug, Deserialize)]
pub struct ModuleExecResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub command_id: Option<String>,
}

// ── Logs ────────────────────────────────────────────

/// BeEF 日志条目.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeefLogEntry {
    pub id: String,
    pub event: String,
    pub timestamp: String,
    pub data: serde_json::Value,
    pub hook_id: Option<String>,
}

// ── BeEF REST Client ────────────────────────────────

/// BeEF REST API 客户端.
#[derive(Clone)]
pub struct BeefClient {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl BeefClient {
    /// 创建 BeEF API 客户端.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: None,
            client: reqwest::Client::new(),
        }
    }

    /// 登录 BeEF.
    pub async fn login(&mut self, username: &str, password: &str) -> Result<(), String> {
        let resp: BeefLoginResponse = self.client
            .post(format!("{}/api/admin/login", self.base_url))
            .json(&BeefLogin {
                username: username.to_string(),
                password: password.to_string(),
            })
            .send()
            .await
            .map_err(|e| format!("BeEF login request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("BeEF login parse failed: {}", e))?;

        if resp.success {
            self.token = resp.token.or(resp.session_id);
            Ok(())
        } else {
            Err("BeEF login failed: invalid credentials".into())
        }
    }

    /// 获取所有钩住的浏览器.
    pub async fn list_hooks(&self) -> Result<Vec<HookedBrowser>, String> {
        let resp: HooksResponse = self.client
            .get(format!("{}/api/hooks", self.base_url))
            .send()
            .await
            .map_err(|e| format!("BeEF hooks request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("BeEF hooks parse failed: {}", e))?;

        if resp.success {
            let mut browsers = Vec::new();
            if let Some(hooks) = resp.hooks {
                for (id, info) in hooks {
                    browsers.push(HookedBrowser {
                        id,
                        ip: info.get("ip").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        browser: info.get("browser").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        version: info.get("version").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        os: info.get("os").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        platform: info.get("platform").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        domain: info.get("domain").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        hooked_at: info.get("hooked_at").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        status: info.get("status").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        session: info.get("session").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    });
                }
            }
            Ok(browsers)
        } else {
            Err("BeEF hooks request failed".into())
        }
    }

    /// 执行 BeEF 模块.
    pub async fn execute_module(
        &self,
        zombie_id: &str,
        module_id: &str,
        options: HashMap<String, serde_json::Value>,
    ) -> Result<ModuleExecResponse, String> {
        let resp: ModuleExecResponse = self.client
            .post(format!("{}/api/modules/{}/{}/execute", self.base_url, zombie_id, module_id))
            .json(&ModuleExecRequest {
                zombie: Some(zombie_id.to_string()),
                module: Some(module_id.to_string()),
                options,
            })
            .send()
            .await
            .map_err(|e| format!("BeEF module exec failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("BeEF module exec parse failed: {}", e))?;

        if resp.success {
            Ok(resp)
        } else {
            Err(format!("BeEF module execution failed: {:?}", resp.result))
        }
    }

    /// 获取 BeEF 服务器状态.
    pub async fn server_status(&self) -> Result<serde_json::Value, String> {
        self.client
            .get(format!("{}/api/status", self.base_url))
            .send()
            .await
            .map_err(|e| format!("BeEF status request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("BeEF status parse failed: {}", e))
    }

    /// 健康检查.
    pub async fn is_alive(&self) -> bool {
        self.client
            .get(format!("{}/api/status", self.base_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}
