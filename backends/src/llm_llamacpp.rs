//! 🦙 llama.cpp 本地推理后端 — 直接加载 GGUF 模型.
//!
//! 通过 `llama-server` 子进程启动本地推理服务，支持完全离线的
//! 本地 LLM 推理，无需外部 API 依赖。
//!
//! ## 前置条件
//!
//! 需要安装 llama.cpp: https://github.com/ggml-org/llama.cpp
//!
//! ```bash
//! # macOS/Linux
//! brew install llama.cpp
//! # 或编译安装
//! git clone https://github.com/ggml-org/llama.cpp
//! cd llama.cpp && make -j
//! ```
//!
//! ## 配置
//!
//! ```yaml
//! llm:
//!   provider: llamacpp
//!   default_model: /path/to/model.gguf
//!   llamacpp_args:
//!     n_gpu_layers: -1
//!     ctx_size: 4096
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::*;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tracing::info;

/// llama.cpp server 的 OpenAI 兼容 API 请求.
#[derive(Serialize)]
struct ChatReq {
    model: String,
    messages: Vec<ChatMsg>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct ChatMsg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResp {
    choices: Vec<Choice>,
    usage: Option<UsageData>,
}

#[derive(Deserialize)]
struct Choice {
    message: MsgContent,
}

#[derive(Deserialize)]
struct MsgContent {
    content: Option<String>,
}

#[derive(Deserialize)]
struct UsageData {
    prompt_tokens: u64,
    completion_tokens: u64,
}

/// llama.cpp 本地推理后端.
pub struct LlamaCppLlm {
    /// llama.cpp server base URL (默认 http://127.0.0.1:8080)
    base_url: String,
    /// 模型路径或名称
    model: String,
    /// HTTP 客户端
    client: HttpClient,
    /// 子进程句柄 (可选 - 由本模块启动时持有)
    process: Mutex<Option<std::process::Child>>,
    /// 用量统计
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
}

impl LlamaCppLlm {
    /// 创建新的 llama.cpp 后端.
    ///
    /// `base_url`: llama-server 地址 (例如 http://127.0.0.1:8080)
    /// `model`: 模型名称或路径
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client: HttpClient::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
            process: Mutex::new(None),
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
        }
    }

    /// 自动启动 llama-server 子进程.
    ///
    /// `model_path`: GGUF 模型文件路径
    /// `port`: 服务端口 (默认 8080)
    /// `n_gpu_layers`: GPU 层数 (-1 = 全部, 0 = CPU only)
    pub fn spawn(model_path: &str, port: u16, n_gpu_layers: i32) -> LsResult<Self> {
        let base_url = format!("http://127.0.0.1:{}", port);

        let mut cmd = std::process::Command::new("llama-server");
        cmd.arg("-m")
            .arg(model_path)
            .arg("--port")
            .arg(port.to_string())
            .arg("--host")
            .arg("127.0.0.1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        if n_gpu_layers != 0 {
            cmd.arg("-ngl").arg(n_gpu_layers.to_string());
        }

        let process = cmd.spawn().map_err(|e| {
            LsError::Plugin(format!(
                "failed to start llama-server: {e}. Is llama.cpp installed?"
            ))
        })?;

        info!("llama-server spawned (PID: {}), waiting for startup...", process.id());

        // 等待服务就绪
        let client = HttpClient::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        let health_url = format!("{}/health", base_url);
        for i in 0..60 {
            if let Ok(resp) = client.get(&health_url).send().await {
                if resp.status().is_success() {
                    info!("llama-server ready after {}s", i);
                    break;
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }

        Ok(Self {
            base_url,
            model: model_path.to_string(),
            client: HttpClient::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
            process: Mutex::new(Some(process)),
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
        })
    }

    /// 停止子进程 (如果是由本模块启动的).
    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.process.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
                info!("llama-server process terminated");
            }
        }
    }
}

impl Drop for LlamaCppLlm {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[async_trait]
impl Llm for LlamaCppLlm {
    async fn invoke(&self, _ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        let messages: Vec<ChatMsg> = request
            .messages
            .iter()
            .map(|m| ChatMsg {
                role: match m.role {
                    LlmRole::System => "system",
                    LlmRole::User => "user",
                    LlmRole::Assistant => "assistant",
                    LlmRole::Tool => "tool",
                }
                .to_string(),
                content: m.content.clone(),
            })
            .collect();

        let body = ChatReq {
            model: self.model.clone(),
            messages,
            stream: false,
            max_tokens: request.max_tokens,
            temperature: request.temperature.map(|t| t as f32),
        };

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LsError::Provider(format!("llama.cpp request failed: {e}")))?
            .json::<ChatResp>()
            .await
            .map_err(|e| LsError::Provider(format!("llama.cpp parse failed: {e}")))?;

        let text = resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let usage = resp.usage.unwrap_or(UsageData {
            prompt_tokens: 0,
            completion_tokens: 0,
        });

        self.prompt_tokens
            .fetch_add(usage.prompt_tokens, Ordering::SeqCst);
        self.completion_tokens
            .fetch_add(usage.completion_tokens, Ordering::SeqCst);

        Ok(LlmResponse {
            text,
            tool_calls: None,
            usage: LlmUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.prompt_tokens + usage.completion_tokens,
            },
            finish_reason: Some("stop".into()),
        })
    }

    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        _request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        Err(LsError::NotImplemented(
            "llama.cpp streaming not yet implemented".into(),
        ))
    }

    async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let mut stats = HashMap::new();
        stats.insert(
            "prompt_tokens".into(),
            self.prompt_tokens.load(Ordering::SeqCst),
        );
        stats.insert(
            "completion_tokens".into(),
            self.completion_tokens.load(Ordering::SeqCst),
        );
        Ok(stats)
    }
}
