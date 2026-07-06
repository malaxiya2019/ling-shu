//! LSMultimodal — Lingshu 多模态支持.
//!
//! 提供图像/音频处理、文件分析管道和多模态 RAG 增强功能。
//!
//! ## 架构
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                MultimodalPipeline                 │
//! │  ┌──────────┐ ┌──────────┐ ┌──────────┐          │
//! │  │ Image    │ │ Audio    │ │ File     │          │
//! │  │ Processor│ │ Processor│ │ Analyzer │          │
//! │  └──────────┘ └──────────┘ └──────────┘          │
//! │  ┌──────────────────────────────────────────┐    │
//! │  │          MultimodalRag                    │    │
//! │  └──────────────────────────────────────────┘    │
//! └──────────────────────────────────────────────────┘
//! ```

pub mod audio;
pub mod file;
pub mod image;
pub mod rag;

pub use audio::{AudioInfo, AudioProcessor};
pub use file::{FileAnalyzer, FileInfo, FileType};
pub use image::{ImageAnalysis, ImageFormat, ImageInfo, ImageProcessor, ImageResizeOptions};
pub use rag::MultimodalRag;
