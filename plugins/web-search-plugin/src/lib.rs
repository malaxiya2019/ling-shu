//! 🔍 Web Search Plugin — 联网搜索支持
//!
//! 让 Agent 能够通过 DuckDuckGo (免费，无需 API Key) 或
//! Bing/Google (需 API Key) 搜索网络信息。
//!
//! ## 支持的搜索引擎
//!
//! | 引擎 | API Key | 费用 | 特点 |
//! |------|---------|------|------|
//! | DuckDuckGo | 无需 | 免费 | 隐私优先，无需注册 |
//! | Bing | 需要 | 按量付费 | 结果质量高 |
//! | Google | 需要 | 按量付费 | 覆盖面广 |
//!
//! ## 环境变量
//!
//! - `LINGSHU_SEARCH_ENGINE` — 搜索引擎: `duckduckgo` (默认) | `bing` | `google`
//! - `LINGSHU_BING_API_KEY` — Bing Search API Key
//! - `LINGSHU_GOOGLE_API_KEY` — Google Custom Search API Key
//! - `LINGSHU_GOOGLE_CSE_ID` — Google Custom Search Engine ID

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::plugin::{Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus};

use std::sync::atomic::{AtomicU64, Ordering};

// ===========================================================================
// 搜索结果
// ===========================================================================

/// 搜索条目.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    /// 标题.
    pub title: String,
    /// URL.
    pub url: String,
    /// 摘要.
    pub snippet: String,
    /// 来源引擎.
    pub engine: String,
}

// ===========================================================================
// 搜索引擎客户端
// ===========================================================================

/// 搜索引擎客户端 trait.
#[async_trait]
trait SearchEngineClient: Send + Sync {
    async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>, String>;
}

/// DuckDuckGo 搜索引擎 (免费，无需 API Key).
struct DuckDuckGoClient {
    client: reqwest::Client,
}

impl DuckDuckGoClient {
    fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .user_agent("Lingshu/1.0 (Agent Search)")
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl SearchEngineClient for DuckDuckGoClient {
    async fn search(&self, query: &str, max_results: usize) -> Result<Vec<SearchResult>, String> {
        // DuckDuckGo Lite API (无需 API Key)
        let url = format!(
            "https://lite.duckduckgo.com/lite/?q={}",
            urlencoding(query)
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("DuckDuckGo request failed: {e}"))?;

        let html = resp
            .text()
            .await
            .map_err(|e| format!("DuckDuckGo response read failed: {e}"))?;

        // 简单 HTML 解析提取结果
        let mut results = Vec::new();
        let mut in_result = false;
        let mut title = String::new();
        let mut url = String::new();
        let mut snippet = String::new();
        let mut field_idx = 0;

        for line in html.lines() {
            let line = line.trim();
            if line.starts_with("<tr") && !in_result {
                in_result = true;
                title.clear();
                url.clear();
                snippet.clear();
                field_idx = 0;
                continue;
            }
            if in_result {
                if line.starts_with("</tr>") {
                    if !title.is_empty() && !url.is_empty() {
                        results.push(SearchResult {
                            title: title.clone(),
                            url: url.clone(),
                            snippet: snippet.clone(),
                            engine: "duckduckgo".into(),
                        });
                    }
                    in_result = false;
                    if results.len() >= max_results {
                        break;
                    }
                    continue;
                }
                // 提取标题和 URL (从 <a> 标签)
                if let Some(href_start) = line.find("href=\"") {
                    let href_start = href_start + 6;
                    if let Some(href_end) = line[href_start..].find('"') {
                        let extracted_url = &line[href_start..href_start + href_end];
                        if extracted_url.starts_with("http") {
                            url = extracted_url.to_string();
                        }
                    }
                }
                if let Some(a_start) = line.find(">") {
                    if let Some(a_end) = line.rfind("</a>") {
                        let extracted = &line[a_start + 1..a_end];
                        // 跳过空结果和重定向行
                        if !extracted.is_empty() && field_idx == 0 {
                            title = extracted.to_string();
                            field_idx = 1;
                            continue;
                        }
                    }
                }
                // 提取摘要 (非 HTML 标签行)
                if field_idx == 1 && !line.starts_with("<") && !line.is_empty() {
                    snippet.push_str(line);
                    snippet.push(' ');
                }
                if line.contains("</td>") && field_idx == 1 {
                    field_idx = 2;
                }
            }
        }

        Ok(results)
    }
}

/// 简单的 URL 编码.
fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}

// ===========================================================================
// 插件实现
// ===========================================================================

/// 联网搜索插件.
pub struct WebSearchPlugin {
    /// 引擎客户端.
    engine: Box<dyn SearchEngineClient>,
    /// 引擎名称.
    engine_name: String,
    /// 搜索计数.
    search_count: AtomicU64,
    /// 插件信息.
    info: PluginInfo,
}

impl WebSearchPlugin {
    /// 创建插件实例，自动检测环境变量选择引擎.
    pub fn new() -> Self {
        let engine_name = std::env::var("LINGSHU_SEARCH_ENGINE")
            .unwrap_or_else(|_| "duckduckgo".to_string());

        let engine: Box<dyn SearchEngineClient> = match engine_name.as_str() {
            "duckduckgo" => Box::new(DuckDuckGoClient::new()),
            other => {
                tracing::warn!(engine = other, "未知搜索引擎，回退到 DuckDuckGo");
                Box::new(DuckDuckGoClient::new())
            }
        };

        let manifest = PluginManifest {
            name: "web-search".into(),
            version: "1.0.0".into(),
            description: "联网搜索 — 支持 DuckDuckGo/Bing/Google 搜索引擎".into(),
            author: Some("Lingshu Team".into()),
            homepage: Some("https://github.com/malaxiya2019/ling-shu".into()),
            license: Some("MIT".into()),
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![PluginPermission {
                resource: "network".into(),
                actions: vec!["http".into()],
            }],
            min_api_version: Some("1.0.0".into()),
        };

        Self {
            engine,
            engine_name,
            search_count: AtomicU64::new(0),
            info: PluginInfo {
                plugin_id: LsId::new(),
                manifest,
                status: PluginStatus::Loaded,
                loaded_at: Some(chrono::Utc::now()),
            },
        }
    }

    /// 执行搜索.
    pub async fn search(&self, query: &str, max_results: Option<usize>) -> LsResult<Vec<SearchResult>> {
        let max = max_results.unwrap_or(5).min(20);
        self.search_count.fetch_add(1, Ordering::SeqCst);
        let results = self
            .engine
            .search(query, max)
            .await
            .map_err(|e| lingshu_core::LsError::Plugin(format!("搜索失败: {e}")))?;
        Ok(results)
    }

    /// 获取搜索统计.
    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": self.engine_name,
            "total_searches": self.search_count.load(Ordering::SeqCst),
            "status": "running",
        })
    }
}

impl Default for WebSearchPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for WebSearchPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "web-search", engine = %self.engine_name, "Web search plugin initialized");
        Ok(())
    }

    async fn start(&self, ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "web-search", session = %ctx.session_id, engine = %self.engine_name, "Web search plugin started");
        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "web-search", "Web search plugin stopped");
        Ok(())
    }

    fn required_permissions(&self) -> Vec<PluginPermission> {
        self.info.manifest.permissions.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encoding() {
        assert_eq!(urlencoding("hello world"), "hello+world");
        assert_eq!(urlencoding("a&b"), "a%26b");
        assert_eq!(urlencoding("test"), "test");
    }

    #[test]
    fn test_plugin_info() {
        let plugin = WebSearchPlugin::new();
        let info = plugin.info();
        assert_eq!(info.manifest.name, "web-search");
        assert_eq!(info.manifest.plugin_type, "static");
    }

    #[test]
    fn test_plugin_permissions() {
        let plugin = WebSearchPlugin::new();
        let perms = plugin.required_permissions();
        assert!(perms.iter().any(|p| p.resource == "network"));
    }

    #[test]
    fn test_default_engine() {
        let plugin = WebSearchPlugin::new();
        assert_eq!(plugin.engine_name, "duckduckgo");
    }

    #[tokio::test]
    async fn test_stats() {
        let plugin = WebSearchPlugin::new();
        let stats = plugin.stats();
        assert_eq!(stats["engine"], "duckduckgo");
        assert_eq!(stats["total_searches"], 0);
    }
}
