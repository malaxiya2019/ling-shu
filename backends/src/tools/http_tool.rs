//! HTTP 网络请求工具 — HttpGetTool, HttpPostTool

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};
use serde_json::Value;

/// HTTP GET 请求工具.
pub struct HttpGetTool;

#[async_trait]
impl Tool for HttpGetTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "http_get".into(),
            description: "发送 HTTP GET 请求并返回响应。用于获取网页内容或调用 REST API。".into(),
            parameters: vec![
                ToolParam {
                    name: "url".into(),
                    description: "请求 URL (必须以 http:// 或 https:// 开头)".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "timeout_secs".into(),
                    description: "超时秒数 (默认 30)".into(),
                    required: false,
                    param_type: "number".into(),
                },
                ToolParam {
                    name: "headers".into(),
                    description: "自定义请求头 (JSON 对象)".into(),
                    required: false,
                    param_type: "object".into(),
                },
            ],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LsError::Validation("missing required field: url".into()))?;

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(LsError::Validation(format!(
                "invalid URL '{url}': must start with http:// or https://"
            )));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        self.validate(&input)?;
        let url = input["url"].as_str().unwrap();
        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .user_agent("LingShu-Agent/1.0")
            .build()
            .map_err(|e| LsError::Internal(format!("failed to build HTTP client: {e}")))?;

        let mut req = client.get(url);

        // 添加自定义请求头
        if let Some(headers) = input.get("headers").and_then(|v| v.as_object()) {
            for (key, val) in headers {
                if let Some(val_str) = val.as_str() {
                    if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes())
                    {
                        if let Ok(header_val) = reqwest::header::HeaderValue::from_str(val_str) {
                            req = req.header(header_name, header_val);
                        }
                    }
                }
            }
        }

        let resp = req
            .send()
            .await
            .map_err(|e| LsError::Internal(format!("HTTP GET '{url}' failed: {e}")))?;

        let status = resp.status().as_u16();
        let headers: serde_json::Map<String, Value> = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    Value::String(v.to_str().unwrap_or("").to_string()),
                )
            })
            .collect();

        let body = resp
            .text()
            .await
            .map_err(|e| LsError::Internal(format!("failed to read response body: {e}")))?;

        Ok(serde_json::json!({
            "url": url,
            "status_code": status,
            "headers": headers,
            "body": body,
            "body_size_bytes": body.len(),
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(HttpGetTool)
    }}

/// HTTP POST 请求工具.
pub struct HttpPostTool;

#[async_trait]
impl Tool for HttpPostTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "http_post".into(),
            description: "发送 HTTP POST 请求并返回响应。用于调用 REST API 发送数据。".into(),
            parameters: vec![
                ToolParam {
                    name: "url".into(),
                    description: "请求 URL".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "body".into(),
                    description: "请求体内容 (JSON 对象或字符串)".into(),
                    required: true,
                    param_type: "object".into(),
                },
                ToolParam {
                    name: "content_type".into(),
                    description: "Content-Type (默认 application/json)".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "timeout_secs".into(),
                    description: "超时秒数 (默认 30)".into(),
                    required: false,
                    param_type: "number".into(),
                },
                ToolParam {
                    name: "headers".into(),
                    description: "自定义请求头 (JSON 对象)".into(),
                    required: false,
                    param_type: "object".into(),
                },
            ],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LsError::Validation("missing required field: url".into()))?;

        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(LsError::Validation(format!(
                "invalid URL '{url}': must start with http:// or https://"
            )));
        }

        if input.get("body").is_none() {
            return Err(LsError::Validation("missing required field: body".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        self.validate(&input)?;
        let url = input["url"].as_str().unwrap();
        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);
        let content_type = input
            .get("content_type")
            .and_then(|v| v.as_str())
            .unwrap_or("application/json");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .user_agent("LingShu-Agent/1.0")
            .build()
            .map_err(|e| LsError::Internal(format!("failed to build HTTP client: {e}")))?;

        let body_value = &input["body"];
        let body_str = if content_type == "application/json"
            || content_type == "application/json; charset=utf-8"
        {
            serde_json::to_string(body_value)
                .map_err(|e| LsError::Validation(format!("failed to serialize body: {e}")))?
        } else {
            body_value.as_str().unwrap_or("").to_string()
        };

        let mut req = client
            .post(url)
            .header("Content-Type", content_type)
            .body(body_str);

        // 添加自定义请求头
        if let Some(headers) = input.get("headers").and_then(|v| v.as_object()) {
            for (key, val) in headers {
                if let Some(val_str) = val.as_str() {
                    if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes())
                    {
                        if let Ok(header_val) = reqwest::header::HeaderValue::from_str(val_str) {
                            req = req.header(header_name, header_val);
                        }
                    }
                }
            }
        }

        let resp = req
            .send()
            .await
            .map_err(|e| LsError::Internal(format!("HTTP POST '{url}' failed: {e}")))?;

        let status = resp.status().as_u16();
        let headers: serde_json::Map<String, Value> = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    Value::String(v.to_str().unwrap_or("").to_string()),
                )
            })
            .collect();

        let body = resp
            .text()
            .await
            .map_err(|e| LsError::Internal(format!("failed to read response body: {e}")))?;

        Ok(serde_json::json!({
            "url": url,
            "status_code": status,
            "headers": headers,
            "body": body,
            "body_size_bytes": body.len(),
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(HttpPostTool)
    }}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use serde_json::json;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[test]
    fn test_validate_http_get() {
        let tool = HttpGetTool;
        assert!(tool
            .validate(&json!({"url": "https://example.com"}))
            .is_ok());
        assert!(tool.validate(&json!({"url": "http://example.com"})).is_ok());
        assert!(tool.validate(&json!({"url": "ftp://bad.com"})).is_err());
        assert!(tool.validate(&json!({"url": ""})).is_err());
    }

    #[test]
    fn test_validate_http_post() {
        let tool = HttpPostTool;
        assert!(tool
            .validate(&json!({"url": "https://api.example.com", "body": {"key": "val"}}))
            .is_ok());
        assert!(tool
            .validate(&json!({"url": "https://api.example.com"}))
            .is_err());
    }

    #[tokio::test]
    async fn test_http_get_timeout() {
        let tool = HttpGetTool;
        // 尝试连接不可达地址，应超时
        let result = tool
            .execute(
                test_ctx(),
                json!({"url": "http://192.0.2.1:9999", "timeout_secs": 1}),
            )
            .await;
        assert!(result.is_err());
    }
}
