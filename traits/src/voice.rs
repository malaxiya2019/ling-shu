//! Voice — 语音合成/识别接口契约.
//!
//! 定义全系统统一的 TTS (文本转语音) 和 STT (语音转文本) 接口。

use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use serde::{Deserialize, Serialize};

/// TTS 请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsRequest {
    /// 要合成的文本.
    pub text: String,
    /// 模型名称 (可选, 如 "tts-1").
    #[serde(default)]
    pub model: Option<String>,
    /// 语音风格/音色 (可选, 如 "alloy", "echo").
    #[serde(default)]
    pub voice: Option<String>,
    /// 语速 (0.25 ~ 4.0, 默认 1.0).
    #[serde(default = "default_speed")]
    pub speed: f64,
    /// 输出音频格式 (可选, 如 "wav", "mp3", "opus").
    #[serde(default = "default_format")]
    pub format: String,
    /// 语言代码 (可选, 如 "zh-CN", "en-US").
    #[serde(default)]
    pub language: Option<String>,
}

fn default_speed() -> f64 { 1.0 }
fn default_format() -> String { "wav".to_string() }

impl Default for TtsRequest {
    fn default() -> Self {
        Self {
            text: String::new(),
            model: None,
            voice: None,
            speed: 1.0,
            format: "wav".to_string(),
            language: None,
        }
    }
}

/// TTS 响应 — 音频数据.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsResponse {
    /// 音频二进制数据 (WAV/MP3/Opus).
    pub audio_data: Vec<u8>,
    /// 音频格式.
    pub format: String,
    /// 音频时长 (秒).
    #[serde(default)]
    pub duration_secs: f64,
    /// 采样率.
    #[serde(default)]
    pub sample_rate: u32,
}

/// STT 请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttRequest {
    /// 音频二进制数据.
    pub audio_data: Vec<u8>,
    /// 音频格式 (如 "wav", "mp3", "ogg").
    pub format: String,
    /// 模型名称 (可选).
    #[serde(default)]
    pub model: Option<String>,
    /// 语言代码 (可选, 如 "zh-CN", "en-US").
    #[serde(default)]
    pub language: Option<String>,
    /// 是否启用标点恢复.
    #[serde(default = "default_true")]
    pub punctuate: bool,
}

fn default_true() -> bool { true }

/// STT 响应 — 转写文本.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttResponse {
    /// 识别出的文本.
    pub text: String,
    /// 识别的语言代码.
    #[serde(default)]
    pub language: Option<String>,
    /// 置信度 (0.0 ~ 1.0).
    #[serde(default)]
    pub confidence: f64,
    /// 各段落的详细信息.
    #[serde(default)]
    pub segments: Vec<SttSegment>,
}

/// STT 段落信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub confidence: f64,
}

/// 文本转语音抽象接口.
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// 提供者名称.
    fn name(&self) -> &str;

    /// 将文本合成为音频.
    async fn synthesize(&self, ctx: LsContext, request: TtsRequest) -> LsResult<TtsResponse>;

    /// 检查提供者是否可用.
    async fn health_check(&self) -> LsResult<bool>;

    /// 列出可用语音风格.
    async fn list_voices(&self) -> LsResult<Vec<String>>;
}

/// 语音转文本抽象接口.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// 提供者名称.
    fn name(&self) -> &str;

    /// 将音频转写为文本.
    async fn transcribe(&self, ctx: LsContext, request: SttRequest) -> LsResult<SttResponse>;

    /// 检查提供者是否可用.
    async fn health_check(&self) -> LsResult<bool>;
}
