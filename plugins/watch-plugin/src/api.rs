//! 📹 Watch Skill REST API 客户端.
//!
//! 包装 watch-skill 的 REST 端点 (FastAPI on :8748).

use serde::{Deserialize, Serialize};

// ── Health ──────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

// ── Watch ───────────────────────────────────────────

/// 观看视频请求.
#[derive(Debug, Serialize)]
pub struct WatchRequest {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<u32>,
    #[serde(default)]
    pub inline_frames: u32,
}

/// 帧信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameInfo {
    pub path: String,
    #[serde(default)]
    pub base64: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
}

/// 观看结果.
#[derive(Debug, Serialize, Deserialize)]
pub struct WatchResponse {
    pub video_id: String,
    pub source: String,
    pub frame_count: u32,
    pub transcript: String,
    pub ocr_text: String,
    #[serde(default)]
    pub frames: Vec<FrameInfo>,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub error: Option<String>,
}

// ── Ask ─────────────────────────────────────────────

/// 询问视频请求.
#[derive(Debug, Serialize)]
pub struct AskRequest {
    pub video: String,
    pub question: String,
    #[serde(default)]
    pub max_frames: u32,
    #[serde(default)]
    pub inline_frames: u32,
}

/// 答案证据.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub timestamp: f64,
    pub kind: String,
    pub text: String,
    pub confidence: f64,
}

/// 询问结果.
#[derive(Debug, Serialize, Deserialize)]
pub struct AskResponse {
    pub answer: String,
    pub confidence: f64,
    pub verified: bool,
    #[serde(default)]
    pub escalations_used: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub frames: Vec<FrameInfo>,
}

// ── Search ──────────────────────────────────────────

/// 搜索结果项.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub video_id: String,
    pub source: String,
    pub timestamp: f64,
    pub text: String,
    pub score: f64,
}

// ── Video list ──────────────────────────────────────

/// 视频列表项.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoItem {
    pub video_id: String,
    pub source: String,
    #[serde(default)]
    pub title: Option<String>,
    pub frame_count: u32,
    pub duration_secs: f64,
    pub indexed_at: String,
}

// ── Capture ─────────────────────────────────────────

/// 捕获请求.
#[derive(Debug, Serialize)]
pub struct CaptureRequest {
    pub target: String,
    #[serde(default)]
    pub duration: f64,
}

/// 捕获结果.
#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureResponse {
    pub video_id: String,
    pub kind: String,
    pub video_path: String,
}

// ── Loop ────────────────────────────────────────────

/// THE LOOP 启动请求.
#[derive(Debug, Serialize)]
pub struct LoopStartRequest {
    pub target: String,
    pub pass_criteria: String,
    #[serde(default)]
    pub max_iterations: u32,
    #[serde(default)]
    pub duration: f64,
}

/// THE LOOP 状态响应.
#[derive(Debug, Serialize, Deserialize)]
pub struct LoopResponse {
    pub loop_id: String,
    pub status: String,
    pub target: String,
    pub iterations: u32,
    pub report: serde_json::Value,
}

// ── Doctor / Diagnostics ────────────────────────────

/// 诊断检查项.
#[derive(Debug, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    #[serde(default)]
    pub fix: Option<String>,
}

/// 诊断结果.
#[derive(Debug, Serialize, Deserialize)]
pub struct DoctorResponse {
    pub status: String,
    pub checks: Vec<DoctorCheck>,
}

// ── HTTP Client ─────────────────────────────────────

/// Watch Skill API HTTP 客户端.
pub struct WatchClient {
    base_url: String,
    client: reqwest::Client,
}

impl WatchClient {
    /// 创建新客户端.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// 健康检查.
    pub async fn health(&self) -> Result<HealthResponse, String> {
        let resp = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| format!("health check failed: {}", e))?;
        resp.json()
            .await
            .map_err(|e| format!("deserialize failed: {}", e))
    }

    /// 观看视频.
    pub async fn watch(&self, req: &WatchRequest) -> Result<WatchResponse, String> {
        let resp = self
            .client
            .post(format!("{}/v1/watch", self.base_url))
            .json(req)
            .send()
            .await
            .map_err(|e| format!("watch request failed: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("watch failed ({status}): {body}"));
        }
        resp.json()
            .await
            .map_err(|e| format!("deserialize failed: {}", e))
    }

    /// 询问视频.
    pub async fn ask(&self, req: &AskRequest) -> Result<AskResponse, String> {
        let resp = self
            .client
            .post(format!("{}/v1/ask", self.base_url))
            .json(req)
            .send()
            .await
            .map_err(|e| format!("ask request failed: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("ask failed ({status}): {body}"));
        }
        resp.json()
            .await
            .map_err(|e| format!("deserialize failed: {}", e))
    }

    /// 搜索视频.
    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, String> {
        let resp = self
            .client
            .get(format!("{}/v1/search", self.base_url))
            .query(&[("q", query)])
            .send()
            .await
            .map_err(|e| format!("search failed: {}", e))?;
        resp.json()
            .await
            .map_err(|e| format!("deserialize failed: {}", e))
    }

    /// 列出视频.
    pub async fn list_videos(&self) -> Result<Vec<VideoItem>, String> {
        let resp = self
            .client
            .get(format!("{}/v1/videos", self.base_url))
            .send()
            .await
            .map_err(|e| format!("list videos failed: {}", e))?;
        resp.json()
            .await
            .map_err(|e| format!("deserialize failed: {}", e))
    }

    /// 捕获屏幕/UI.
    pub async fn capture(&self, req: &CaptureRequest) -> Result<CaptureResponse, String> {
        let resp = self
            .client
            .post(format!("{}/v1/capture", self.base_url))
            .json(req)
            .send()
            .await
            .map_err(|e| format!("capture failed: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("capture failed ({status}): {body}"));
        }
        resp.json()
            .await
            .map_err(|e| format!("deserialize failed: {}", e))
    }

    /// 启动 THE LOOP.
    pub async fn loop_start(&self, req: &LoopStartRequest) -> Result<LoopResponse, String> {
        let resp = self
            .client
            .post(format!("{}/v1/loops", self.base_url))
            .json(req)
            .send()
            .await
            .map_err(|e| format!("loop start failed: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("loop start failed ({status}): {body}"));
        }
        resp.json()
            .await
            .map_err(|e| format!("deserialize failed: {}", e))
    }

    /// THE LOOP 迭代.
    pub async fn loop_iterate(&self, loop_id: &str) -> Result<LoopResponse, String> {
        let resp = self
            .client
            .post(format!("{}/v1/loops/{}/iterate", self.base_url, loop_id))
            .send()
            .await
            .map_err(|e| format!("loop iterate failed: {}", e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("loop iterate failed ({status}): {body}"));
        }
        resp.json()
            .await
            .map_err(|e| format!("deserialize failed: {}", e))
    }

    /// THE LOOP 状态.
    pub async fn loop_status(&self, loop_id: &str) -> Result<LoopResponse, String> {
        let resp = self
            .client
            .get(format!("{}/v1/loops/{}", self.base_url, loop_id))
            .send()
            .await
            .map_err(|e| format!("loop status failed: {}", e))?;
        resp.json()
            .await
            .map_err(|e| format!("deserialize failed: {}", e))
    }
}
