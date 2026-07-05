//! 音频处理模块 — 音频信息提取与分析.

use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};

/// 音频信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioInfo {
    /// 文件格式 (如 mp3, wav, ogg)
    pub format: String,
    /// 文件大小 (字节)
    pub size_bytes: u64,
    /// 时长 (秒), 未知时为 None
    pub duration_secs: Option<f64>,
    /// 采样率 (Hz), 未知时为 None
    pub sample_rate: Option<u32>,
    /// 声道数, 未知时为 None
    pub channels: Option<u32>,
    /// MIME 类型
    pub mime_type: String,
}

/// 音频处理器.
pub struct AudioProcessor;

impl AudioProcessor {
    /// 分析音频文件信息 (基于文件头/metadata).
    pub fn analyze(data: &[u8]) -> LsResult<AudioInfo> {
        let (format, mime) = detect_audio_format(data);
        let size_bytes = data.len() as u64;

        Ok(AudioInfo {
            format: format.to_string(),
            size_bytes,
            duration_secs: None, // 需要完整解码才能获取
            sample_rate: None,
            channels: None,
            mime_type: mime.to_string(),
        })
    }

    /// 获取音频 MIME 类型.
    pub fn detect_mime(data: &[u8]) -> &'static str {
        detect_audio_format(data).1
    }

    /// 将音频数据编码为 Base64 data URL.
    pub fn to_data_url(data: &[u8]) -> String {
        let (_, mime) = detect_audio_format(data);
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(data);
        format!("data:{};base64,{}", mime, b64)
    }
}

/// 通过 magic bytes 检测音频格式.
fn detect_audio_format(data: &[u8]) -> (&'static str, &'static str) {
    if data.len() < 4 {
        return ("unknown", "application/octet-stream");
    }

    // MP3: FF FB 或 FF F3 或 FF F2
    if data[0] == 0xFF && (data[1] & 0xF0) == 0xF0 {
        return ("mp3", "audio/mpeg");
    }
    // WAV: 52 49 46 46 (RIFF)
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        return ("wav", "audio/wav");
    }
    // OGG: 4F 67 67 53 (OggS)
    if &data[0..4] == b"OggS" {
        return ("ogg", "audio/ogg");
    }
    // FLAC: 66 4C 61 43 (fLaC)
    if &data[0..4] == b"fLaC" {
        return ("flac", "audio/flac");
    }
    // WebM: 1A 45 DF A3
    if data[0] == 0x1A && data[1] == 0x45 && data[2] == 0xDF && data[3] == 0xA3 {
        return ("webm", "audio/webm");
    }
    // AAC: FF F1 或 FF F9
    if data[0] == 0xFF && (data[1] & 0xF6) == 0xF0 {
        return ("aac", "audio/aac");
    }

    ("unknown", "application/octet-stream")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_mp3() {
        let data = vec![0xFF, 0xFB, 0x90, 0x00];
        let (fmt, mime) = detect_audio_format(&data);
        assert_eq!(fmt, "mp3");
        assert_eq!(mime, "audio/mpeg");
    }

    #[test]
    fn test_detect_wav() {
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&[0u8; 4]);
        data.extend_from_slice(b"WAVE");
        let (fmt, _) = detect_audio_format(&data);
        assert_eq!(fmt, "wav");
    }

    #[test]
    fn test_detect_ogg() {
        let data = b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        let (fmt, _) = detect_audio_format(&data);
        assert_eq!(fmt, "ogg");
    }

    #[test]
    fn test_detect_flac() {
        let data = b"fLaC\x00\x00\x00\x22\x12\x00\x12\x00".to_vec();
        let (fmt, _) = detect_audio_format(&data);
        assert_eq!(fmt, "flac");
    }

    #[test]
    fn test_detect_unknown() {
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let (fmt, _) = detect_audio_format(&data);
        assert_eq!(fmt, "unknown");
    }

    #[test]
    fn test_analyze_audio() {
        let data = vec![0xFF, 0xFB, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00];
        let info = AudioProcessor::analyze(&data).unwrap();
        assert_eq!(info.format, "mp3");
        assert_eq!(info.size_bytes, 8);
        assert!(info.duration_secs.is_none());
    }

    #[test]
    fn test_data_url() {
        let data = vec![0xFF, 0xFB, 0x90, 0x00];
        let url = AudioProcessor::to_data_url(&data);
        assert!(url.starts_with("data:audio/mpeg;base64,"));
    }
}
