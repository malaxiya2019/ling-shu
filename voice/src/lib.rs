//! Lingshu Voice — 多模态语音 Tool.
//!
//! 提供:
//! - `OmniVoiceClient` — 封装 OmniVoice Studio HTTP API 的 TTS/STT 客户端
//! - `VoiceTool` — Agent 可调用的语音合成/识别 Tool
//!
//! # 环境变量
//!
//! - `OMNIVOICE_API_URL` — OmniVoice 服务地址 (默认: http://localhost:3900)

mod client;
mod tools;

pub use client::OmniVoiceClient;
pub use tools::{SayTool, ListenTool, VoiceTools};
pub use lingshu_traits::voice::{
    SttProvider, SttRequest, SttResponse, SttSegment,
    TtsProvider, TtsRequest, TtsResponse,
};
