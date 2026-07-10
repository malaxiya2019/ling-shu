//! API client — talks to the Lingshu HTTP backend.

use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

// ── Response types mirroring the backend API ──────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime: String,
    pub checks: Vec<HealthCheckItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthCheckItem {
    pub name: String,
    pub healthy: bool,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FederationStatus {
    pub cluster_id: String,
    pub cluster_name: String,
    pub enabled: bool,
    pub node_count: usize,
    pub uptime_secs: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FederationNodeInfo {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub last_seen: String,
    #[serde(default)]
    pub cluster_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvalResultSummary {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub metrics: std::collections::HashMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    #[serde(default)]
    pub build_date: String,
    #[serde(default)]
    pub commit: String,
}

// ── API calls ─────────────────────────────────────────

pub async fn get_health() -> Result<HealthResponse, String> {
    get_json::<HealthResponse>("/health").await
}

pub async fn get_federation_status() -> Result<FederationStatus, String> {
    get_json::<FederationStatus>("/v1/federation/status").await
}

pub async fn get_federation_nodes() -> Result<Vec<FederationNodeInfo>, String> {
    get_json::<Vec<FederationNodeInfo>>("/v1/federation/nodes").await
}

pub async fn get_eval_result() -> Result<EvalResultSummary, String> {
    get_json::<EvalResultSummary>("/v1/eval/result").await
}

pub async fn get_version() -> Result<VersionInfo, String> {
    get_json::<VersionInfo>("/version").await
}

pub(crate) async fn get_json<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let resp = Request::get(path)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.json::<T>()
        .await
        .map_err(|e| format!("deserialize failed: {e}"))
}

/// 检查当前登录状态.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub roles: Option<Vec<String>>,
}

pub async fn get_auth_me() -> Result<AuthStatus, String> {
    get_json::<AuthStatus>("/api/auth/me").await
}

// ── Plugin Market API types ──────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginListItem {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    pub plugin_type: String,
    pub status: String,
    #[serde(default)]
    pub loaded_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginListResponse {
    pub plugins: Vec<PluginListItem>,
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketPluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub download_url: String,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketSearchResponse {
    pub query: String,
    pub total: usize,
    pub plugins: Vec<MarketPluginEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketInstallRequest {
    pub name: String,
    pub version: String,
    pub download_url: String,
    #[serde(default)]
    pub checksum: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketInstallResponse {
    pub name: String,
    pub version: String,
    pub path: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginActionResponse {
    pub id: String,
    pub name: String,
    pub status: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketSourceItem {
    pub source_type: String,
    pub source_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketAddSourceRequest {
    pub source_type: String,
    pub source_url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginInstallRequest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub plugin_type: Option<String>,
    #[serde(default)]
    pub permissions: Option<Vec<PluginPermission>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginPermission {
    pub resource: String,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginInstallResponse {
    pub id: String,
    pub name: String,
    pub status: String,
}

// ── Plugin Market API calls ──────────────────────────

pub async fn get_plugins() -> Result<PluginListResponse, String> {
    get_json::<PluginListResponse>("/v1/plugins").await
}

pub async fn get_plugin(id: &str) -> Result<PluginListItem, String> {
    get_json::<PluginListItem>(&format!("/v1/plugins/{id}")).await
}

pub async fn install_plugin(req: &PluginInstallRequest) -> Result<PluginInstallResponse, String> {
    let body = serde_json::to_string(req).map_err(|e| format!("serialize: {e}"))?;
    let resp = Request::post("/v1/plugins")
        .header("Content-Type", "application/json")
        .body(body)
        .map_err(|e| format!("body: {e:?}"))?
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<PluginInstallResponse>()
        .await
        .map_err(|e| format!("deserialize: {e}"))
}

pub async fn start_plugin(id: &str) -> Result<PluginActionResponse, String> {
    let resp = Request::post(&format!("/v1/plugins/{id}/start"))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<PluginActionResponse>()
        .await
        .map_err(|e| format!("deserialize: {e}"))
}

pub async fn stop_plugin(id: &str) -> Result<PluginActionResponse, String> {
    let resp = Request::post(&format!("/v1/plugins/{id}/stop"))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<PluginActionResponse>()
        .await
        .map_err(|e| format!("deserialize: {e}"))
}

pub async fn uninstall_plugin(id: &str) -> Result<(), String> {
    let resp = Request::delete(&format!("/v1/plugins/{id}"))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

pub async fn market_search(query: &str) -> Result<MarketSearchResponse, String> {
    let q = if query.is_empty() { "" } else { query };
    get_json::<MarketSearchResponse>(&format!("/v1/plugins/market/search?q={q}")).await
}

pub async fn market_install(req: &MarketInstallRequest) -> Result<MarketInstallResponse, String> {
    let body = serde_json::to_string(req).map_err(|e| format!("serialize: {e}"))?;
    let resp = Request::post("/v1/plugins/market/install")
        .header("Content-Type", "application/json")
        .body(body)
        .map_err(|e| format!("body: {e:?}"))?
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        let status = resp.status();
        return Err(format!("HTTP {status}"));
    }
    resp.json::<MarketInstallResponse>()
        .await
        .map_err(|e| format!("deserialize: {e}"))
}

pub async fn market_sources() -> Result<Vec<MarketSourceItem>, String> {
    get_json::<Vec<MarketSourceItem>>("/v1/plugins/market/sources").await
}

pub async fn market_add_source(req: &MarketAddSourceRequest) -> Result<(), String> {
    let body = serde_json::to_string(req).map_err(|e| format!("serialize: {e}"))?;
    let resp = Request::post("/v1/plugins/market/sources")
        .header("Content-Type", "application/json")
        .body(body)
        .map_err(|e| format!("body: {e:?}"))?
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

pub async fn hot_reload_start() -> Result<(), String> {
    let resp = Request::post("/v1/plugins/hotreload/start")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

pub async fn hot_reload_stop() -> Result<(), String> {
    let resp = Request::post("/v1/plugins/hotreload/stop")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

// ── BeEF Security Testing API types ──────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeefStatusResponse {
    pub status: String,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub uptime_secs: Option<u64>,
    #[serde(default)]
    pub hooked_browsers: Option<usize>,
    #[serde(default)]
    pub modules_count: Option<usize>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeefActionResponse {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeefHookedBrowser {
    pub id: String,
    pub ip: String,
    pub browser: String,
    pub os: String,
    pub hooked_at: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub page_uri: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeefHooksResponse {
    pub browsers: Vec<BeefHookedBrowser>,
    pub total: usize,
}

pub async fn beef_status() -> Result<BeefStatusResponse, String> {
    get_json::<BeefStatusResponse>("/v1/security/beef/status").await
}

pub async fn beef_start() -> Result<BeefActionResponse, String> {
    let resp = Request::post("/v1/security/beef/start")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<BeefActionResponse>()
        .await
        .map_err(|e| format!("deserialize: {e}"))
}

pub async fn beef_stop() -> Result<BeefActionResponse, String> {
    let resp = Request::post("/v1/security/beef/stop")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<BeefActionResponse>()
        .await
        .map_err(|e| format!("deserialize: {e}"))
}

pub async fn beef_restart() -> Result<BeefActionResponse, String> {
    let resp = Request::post("/v1/security/beef/restart")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<BeefActionResponse>()
        .await
        .map_err(|e| format!("deserialize: {e}"))
}

pub async fn beef_hooks() -> Result<BeefHooksResponse, String> {
    get_json::<BeefHooksResponse>("/v1/security/beef/hooks").await
}

// ── Runtime API types (v4.0+) ─────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStatusResponse {
    pub state: String,
    pub agent_count: usize,
    pub session_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentSummaryItem {
    pub agent_id: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentListResponse {
    pub agents: Vec<AgentSummaryItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionInfoItem {
    pub session_id: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionInfoItem>,
}

// ── Runtime API calls ─────────────────────────────────

pub async fn get_runtime_status() -> Result<RuntimeStatusResponse, String> {
    get_json::<RuntimeStatusResponse>("/api/v1/runtime/status").await
}

pub async fn get_agents() -> Result<AgentListResponse, String> {
    get_json::<AgentListResponse>("/api/v1/agents").await
}

pub async fn get_sessions() -> Result<SessionListResponse, String> {
    get_json::<SessionListResponse>("/api/v1/sessions").await
}
