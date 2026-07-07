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
