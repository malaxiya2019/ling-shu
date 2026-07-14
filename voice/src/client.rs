//! OmniVoice HTTP 客户端 — 封装 TTS / STT API.

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::voice::{
    SttProvider, SttRequest, SttResponse, SttSegment, TtsProvider, TtsRequest, TtsResponse,
};
use serde_json::Value;

/// OmniVoice API HTTP 客户端.
pub struct OmniVoiceClient {
    base_url: String,
    client: reqwest::Client,
}

impl OmniVoiceClient {
    /// 创建新的 OmniVoice 客户端.
    ///
    /// `base_url` 默认为 `OMNIVOICE_API_URL` 环境变量或 `http://localhost:3900`.
    pub fn new(base_url: Option<String>) -> Self {
        let url = base_url
            .or_else(|| std::env::var("OMNIVOICE_API_URL").ok())
            .unwrap_or_else(|| "http://localhost:3900".into());
        Self {
            base_url: url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("OmniVoiceClient: reqwest Client creation"),
        }
    }
}

#[async_trait]
impl TtsProvider for OmniVoiceClient {
    fn name(&self) -> &str {
        "omnivoice"
    }

    async fn synthesize(&self, _ctx: LsContext, request: TtsRequest) -> LsResult<TtsResponse> {
        let url = format!("{}/v1/audio/speech", self.base_url);

        let payload = serde_json::json!({
            "input": request.text,
            "model": request.model.unwrap_or_else(|| "tts-1".into()),
            "voice": request.voice.unwrap_or_else(|| "default".into()),
            "response_format": request.format,
            "speed": request.speed,
        });

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| LsError::External(format!("OmniVoice TTS request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LsError::External(format!(
                "OmniVoice TTS error ({}): {}",
                status.as_u16(),
                body
            )));
        }

        let audio_bytes = resp
            .bytes()
            .await
            .map_err(|e| LsError::External(format!("OmniVoice read audio failed: {e}")))?
            .to_vec();

        Ok(TtsResponse {
            audio_data: audio_bytes,
            format: request.format.clone(),
            duration_secs: 0.0, // 服务端未返回时长，由调用方自行计算
            sample_rate: 24000,
        })
    }

    async fn health_check(&self) -> LsResult<bool> {
        let url = format!("{}/health", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    async fn list_voices(&self) -> LsResult<Vec<String>> {
        let url = format!("{}/v1/audio/voices", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let data: Value = resp
                    .json()
                    .await
                    .map_err(|e| LsError::External(format!("parse voices failed: {e}")))?;
                let voices = data["voices"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(voices)
            }
            Ok(_) => Ok(vec!["default".into()]),
            Err(_) => Ok(vec!["default".into()]),
        }
    }
}

#[async_trait]
impl SttProvider for OmniVoiceClient {
    fn name(&self) -> &str {
        "omnivoice"
    }

    async fn transcribe(&self, _ctx: LsContext, request: SttRequest) -> LsResult<SttResponse> {
        let url = format!("{}/v1/audio/transcriptions", self.base_url);

        let mut form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(request.audio_data)
                    .file_name(format!("audio.{}", request.format))
                    .mime_str("audio/wav")
                    .unwrap_or_else(|_| reqwest::multipart::Part::bytes(vec![])),
            )
            .text("model", request.model.unwrap_or_else(|| "whisper-1".into()));

        if let Some(ref lang) = request.language {
            form = form.text("language", lang.clone());
        }

        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| LsError::External(format!("OmniVoice STT request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LsError::External(format!(
                "OmniVoice STT error ({}): {}",
                status.as_u16(),
                body
            )));
        }

        let data: Value = resp
            .json()
            .await
            .map_err(|e| LsError::External(format!("OmniVoice parse response failed: {e}")))?;

        let text = data["text"].as_str().unwrap_or("").to_string();
        let language = data["language"].as_str().map(String::from);
        let confidence = data["confidence"].as_f64().unwrap_or(0.0);

        let segments = data["segments"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| {
                        Some(SttSegment {
                            start: s["start"].as_f64()?,
                            end: s["end"].as_f64()?,
                            text: s["text"].as_str()?.to_string(),
                            confidence: s["confidence"].as_f64().unwrap_or(0.0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(SttResponse {
            text,
            language,
            confidence,
            segments,
        })
    }

    async fn health_check(&self) -> LsResult<bool> {
        // 同 TTS health check
        self.synthesize(
            LsContext::with_session(lingshu_core::LsId::new()),
            TtsRequest {
                text: "test".into(),
                ..Default::default()
            },
        )
        .await
        .map(|_| true)
        .or(Ok(false))
    }
}
