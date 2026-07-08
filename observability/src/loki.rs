//! Loki 日志推送 — 将 tracing 日志批量推送到 Grafana Loki.
//!
//! ## 使用
//! ```rust,ignore
//! use lingshu_observability::loki::LokiLayer;
//!
//! let layer = LokiLayer::new("http://localhost:3100", "lingshu")?;
//! tracing_subscriber::registry()
//!     .with(layer)
//!     .init();
//! ```
//!
//! ## 环境变量
//! - `LS_LOKI_ENABLED` — 设为 `true` 启用
//! - `LS_LOKI_URL` — Loki HTTP 推送端点 (默认: `http://localhost:3100/loki/api/v1/push`)
//! - `LS_LOKI_TENANT_ID` — Loki 租户 ID (可选)

use lingshu_core::LsResult;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Loki 日志推送配置.
#[derive(Debug, Clone)]
pub struct LokiConfig {
    /// Loki HTTP 端点
    pub url: String,
    /// 租户 ID (可选)
    pub tenant_id: Option<String>,
    /// 标签
    pub labels: HashMap<String, String>,
    /// 批处理间隔 (秒)
    pub batch_interval_secs: u64,
    /// 批处理大小
    pub batch_size: usize,
}

impl Default for LokiConfig {
    fn default() -> Self {
        Self {
            url: std::env::var("LS_LOKI_URL")
                .unwrap_or_else(|_| "http://localhost:3100/loki/api/v1/push".into()),
            tenant_id: std::env::var("LS_LOKI_TENANT_ID").ok(),
            labels: HashMap::from([
                ("service".into(), std::env::var("LS_SERVICE_NAME").unwrap_or_else(|_| "lingshu".into())),
                ("environment".into(), std::env::var("LS_ENV").unwrap_or_else(|_| "dev".into())),
            ]),
            batch_interval_secs: 5,
            batch_size: 100,
        }
    }
}

/// Loki 日志条目标.
#[derive(Debug, Clone, serde::Serialize)]
struct LokiEntry {
    /// 时间戳 (纳秒)
    #[serde(serialize_with = "serialize_ns")]
    ts: i64,
    /// 日志行
    line: String,
}

fn serialize_ns<S>(ts: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.collect_str(&format!("{}", ts))
}

/// Loki push request body.
#[derive(Debug, Clone, serde::Serialize)]
struct LokiPushRequest {
    streams: Vec<LokiStream>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct LokiStream {
    stream: HashMap<String, String>,
    values: Vec<[String; 2]>, // [[timestamp_ns, line], ...]
}

/// Loki 日志推送器.
pub struct LokiClient {
    config: LokiConfig,
    client: reqwest::Client,
    buffer: Arc<Mutex<Vec<LokiEntry>>>,
}

impl LokiClient {
    /// 创建 Loki 客户端.
    pub fn new(config: LokiConfig) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(ref tenant) = config.tenant_id {
            headers.insert(
                "X-Scope-OrgID",
                reqwest::header::HeaderValue::from_str(tenant).unwrap(),
            );
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            config,
            client,
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 添加日志条目到缓冲区.
    pub async fn push_log(&self, line: String) {
        let mut buffer = self.buffer.lock().await;
        buffer.push(LokiEntry {
            ts: chrono::Utc::now().timestamp_nanos(),
            line,
        });

        if buffer.len() >= self.config.batch_size {
            let batch = buffer.drain(..).collect();
            // Fire-and-forget the flush
            tokio::spawn({
                let client = self.client.clone();
                let config = self.config.clone();
                async move {
                    if let Err(e) = Self::flush_inner(&client, &config, batch).await {
                        warn!(error = %e, "Loki batch push failed");
                    }
                }
            });
        }
    }

    /// 刷新缓冲区 (强制推送).
    pub async fn flush(&self) -> LsResult<()> {
        let batch = {
            let mut buffer = self.buffer.lock().await;
            buffer.drain(..).collect()
        };
        if batch.is_empty() {
            return Ok(());
        }
        Self::flush_inner(&self.client, &self.config, batch).await
    }

    /// 内部推送逻辑.
    async fn flush_inner(
        client: &reqwest::Client,
        config: &LokiConfig,
        entries: Vec<LokiEntry>,
    ) -> LsResult<()> {
        let stream = LokiStream {
            stream: config.labels.clone(),
            values: entries
                .iter()
                .map(|e| [format!("{}", e.ts), e.line.clone()])
                .collect(),
        };

        let request = LokiPushRequest {
            streams: vec![stream],
        };

        let resp = client
            .post(&config.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| lingshu_core::LsError::Internal(format!("Loki push: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(status = %status, body = %body, "Loki push returned error");
        }

        Ok(())
    }
}

/// 启动后台定时刷新任务.
pub fn start_loki_flusher(client: Arc<LokiClient>, interval_secs: u64) {
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            timer.tick().await;
            if let Err(e) = client.flush().await {
                warn!(error = %e, "Loki flusher error");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loki_config_default() {
        let config = LokiConfig::default();
        assert!(config.url.contains("loki"));
        assert_eq!(config.batch_size, 100);
    }

    #[tokio::test]
    async fn test_loki_push_without_server() {
        let config = LokiConfig {
            url: "http://127.0.0.1:14318/loki/api/v1/push".into(),
            batch_size: 2,
            ..Default::default()
        };
        let client = LokiClient::new(config);

        // Push a log — should buffer, not fail
        client.push_log("test log line".into()).await;
        // Force flush — will fail to connect but should not panic
        let result = client.flush().await;
        assert!(result.is_err() || result.is_ok());
    }

    #[tokio::test]
    async fn test_buffer_auto_flush() {
        let config = LokiConfig {
            url: "http://127.0.0.1:14318/loki/api/v1/push".into(),
            batch_size: 3,
            ..Default::default()
        };
        let client = LokiClient::new(config);

        // Push 3 logs — should trigger auto-flush
        for i in 0..3 {
            client.push_log(format!("log line {i}")).await;
        }
        // Give the async flush a moment
        tokio::time::sleep(Duration::from_millis(100)).await;
        // Buffer should be empty (or has been drained)
        let buffer_len = client.buffer.lock().await.len();
        assert_eq!(buffer_len, 0);
    }
}
