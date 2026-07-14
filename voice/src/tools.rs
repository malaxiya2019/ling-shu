//! Voice Tools — Agent 可调用的语音合成/识别工具.

use std::sync::Arc;

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_tool::ToolRegistry;
use lingshu_traits::tool::{
    PermissionLevel, Tool, ToolCategory, ToolInfo, ToolMetadata, ToolParam,
};
use lingshu_traits::voice::{SttProvider, SttRequest, TtsProvider, TtsRequest};
use serde_json::Value;

use crate::OmniVoiceClient;

/// TTS 工具 — Agent 调用 "say" 输出语音.
pub struct SayTool {
    tts: Arc<dyn TtsProvider>,
}

impl SayTool {
    pub fn new(tts: Arc<dyn TtsProvider>) -> Self {
        Self { tts }
    }
}

#[async_trait]
impl Tool for SayTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "say".into(),
            description: "将文本合成为语音并输出。用于需要语音回复的场景。支持多语言和多种音色。"
                .into(),
            parameters: vec![
                ToolParam {
                    name: "text".into(),
                    description: "要朗读的文本内容".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "voice".into(),
                    description: "语音风格/音色 (可选)".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "language".into(),
                    description: "语言代码, 如 zh-CN, en-US (可选)".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "speed".into(),
                    description: "语速 0.25~4.0 (默认 1.0)".into(),
                    required: false,
                    param_type: "number".into(),
                },
            ],
            metadata: ToolMetadata {
                category: ToolCategory::AI,
                tags: vec!["voice".into(), "tts".into(), "audio".into()],
                permission_level: PermissionLevel::User,
                timeout_ms: Some(30000),
                ..Default::default()
            },
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if input
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        {
            return Err(LsError::InvalidArgument(
                "'text' is required and cannot be empty".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, ctx: LsContext, input: Value) -> LsResult<Value> {
        let text = input["text"].as_str().unwrap_or("").to_string();
        let voice = input["voice"].as_str().map(String::from);
        let language = input["language"].as_str().map(String::from);
        let speed = input["speed"].as_f64().unwrap_or(1.0);

        let request = TtsRequest {
            text,
            voice,
            language,
            speed,
            ..Default::default()
        };

        let response = self.tts.synthesize(ctx, request).await?;

        // 返回 Base64 编码的音频数据
        let b64 = base64_encode(&response.audio_data);

        Ok(serde_json::json!({
            "audio_base64": b64,
            "format": response.format,
            "duration_secs": response.duration_secs,
            "sample_rate": response.sample_rate,
            "size_bytes": response.audio_data.len(),
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            tts: self.tts.clone(),
        })
    }
}

/// STT 工具 — Agent 调用 "listen" 接收语音输入.
pub struct ListenTool {
    stt: Arc<dyn SttProvider>,
}

impl ListenTool {
    pub fn new(stt: Arc<dyn SttProvider>) -> Self {
        Self { stt }
    }
}

#[async_trait]
impl Tool for ListenTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "listen".into(),
            description: "将语音音频转写为文本。用于处理用户语音输入。支持多语言。".into(),
            parameters: vec![
                ToolParam {
                    name: "audio_base64".into(),
                    description: "Base64 编码的音频数据".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "format".into(),
                    description: "音频格式, 如 wav, mp3, ogg (默认 wav)".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "language".into(),
                    description: "语言代码, 如 zh-CN, en-US (可选)".into(),
                    required: false,
                    param_type: "string".into(),
                },
            ],
            metadata: ToolMetadata {
                category: ToolCategory::AI,
                tags: vec!["voice".into(), "stt".into(), "audio".into()],
                permission_level: PermissionLevel::User,
                timeout_ms: Some(30000),
                ..Default::default()
            },
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if input
            .get("audio_base64")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        {
            return Err(LsError::InvalidArgument(
                "'audio_base64' is required".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, ctx: LsContext, input: Value) -> LsResult<Value> {
        let b64 = input["audio_base64"].as_str().unwrap_or("");
        let format = input["format"].as_str().unwrap_or("wav").to_string();
        let language = input["language"].as_str().map(String::from);

        let audio_data = base64_decode(b64)
            .map_err(|e| LsError::InvalidArgument(format!("base64 decode failed: {e}")))?;

        let request = SttRequest {
            audio_data,
            format,
            language,
            model: None,
            punctuate: true,
        };

        let response = self.stt.transcribe(ctx, request).await?;

        Ok(serde_json::json!({
            "text": response.text,
            "language": response.language,
            "confidence": response.confidence,
            "segments": response.segments.iter().map(|s| serde_json::json!({
                "start": s.start,
                "end": s.end,
                "text": s.text,
                "confidence": s.confidence,
            })).collect::<Vec<_>>(),
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            stt: self.stt.clone(),
        })
    }
}

/// Voice 工具集合 — 提供批量注册辅助函数.
pub struct VoiceTools;

impl VoiceTools {
    /// 创建默认的语音工具集 (基于 OmniVoice 客户端).
    pub fn create_default() -> (SayTool, ListenTool) {
        let client = Arc::new(OmniVoiceClient::new(None));
        (SayTool::new(client.clone()), ListenTool::new(client))
    }

    /// 注册所有语音工具到 ToolRegistry.
    pub async fn register_all(registry: &ToolRegistry) {
        let client = Arc::new(OmniVoiceClient::new(None));
        registry
            .register(Box::new(SayTool::new(client.clone())))
            .await;
        registry.register(Box::new(ListenTool::new(client))).await;
    }

    /// 使用自定义提供者注册语音工具到 ToolRegistry.
    pub async fn register_with_providers(
        registry: &ToolRegistry,
        tts: Arc<dyn TtsProvider>,
        stt: Arc<dyn SttProvider>,
    ) {
        registry.register(Box::new(SayTool::new(tts))).await;
        registry.register(Box::new(ListenTool::new(stt))).await;
    }
}

// ── Base64 辅助函数 ──

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(data: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(data)
}
