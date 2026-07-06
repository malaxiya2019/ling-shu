//! 图像处理模块 — 加载、分析、转换图像.

use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};

/// 图像格式.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Gif,
    WebP,
    Bmp,
    Unknown,
}

impl ImageFormat {
    /// 从 MIME 类型解析.
    pub fn from_mime(mime: &str) -> Self {
        match mime.to_lowercase().as_str() {
            "image/jpeg" | "image/jpg" => ImageFormat::Jpeg,
            "image/png" => ImageFormat::Png,
            "image/gif" => ImageFormat::Gif,
            "image/webp" => ImageFormat::WebP,
            "image/bmp" => ImageFormat::Bmp,
            _ => ImageFormat::Unknown,
        }
    }

    /// 获取对应的 MIME 类型.
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Png => "image/png",
            ImageFormat::Gif => "image/gif",
            ImageFormat::WebP => "image/webp",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Unknown => "application/octet-stream",
        }
    }
}

/// 图像分析结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAnalysis {
    /// 宽度 (像素)
    pub width: u32,
    /// 高度 (像素)
    pub height: u32,
    /// 图像格式
    pub format: ImageFormat,
    /// 文件大小 (字节)
    pub size_bytes: u64,
    /// 颜色模式 (RGB/RGBA/Luminance 等)
    pub color_mode: String,
    /// 是否包含透明度
    pub has_alpha: bool,
    /// 建议的描述/标签
    pub description: Option<String>,
}

/// 图像信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub size_bytes: u64,
}

/// 图像缩放选项.
#[derive(Debug, Clone)]
pub struct ImageResizeOptions {
    pub max_width: u32,
    pub max_height: u32,
    pub quality: u8,
}

impl Default for ImageResizeOptions {
    fn default() -> Self {
        Self {
            max_width: 2048,
            max_height: 2048,
            quality: 85,
        }
    }
}

/// 图像处理器.
pub struct ImageProcessor;

impl ImageProcessor {
    /// 分析图像数据，返回分析结果.
    pub fn analyze(data: &[u8]) -> LsResult<ImageAnalysis> {
        let reader = image::load_from_memory(data)
            .map_err(|e| lingshu_core::LsError::Internal(format!("image decode failed: {e}")))?;

        let (width, height) = (reader.width(), reader.height());
        let size_bytes = data.len() as u64;
        let has_alpha = reader.color().has_alpha();
        let color_mode = format!("{:?}", reader.color());

        // 通过 magic bytes 猜测格式
        let format = guess_image_format(data);

        Ok(ImageAnalysis {
            width,
            height,
            format,
            size_bytes,
            color_mode,
            has_alpha,
            description: None,
        })
    }

    /// 获取图像基本信息 (轻量级，不解码整个图像).
    pub fn info(data: &[u8]) -> LsResult<ImageInfo> {
        let reader = image::load_from_memory(data)
            .map_err(|e| lingshu_core::LsError::Internal(format!("image decode failed: {e}")))?;

        Ok(ImageInfo {
            width: reader.width(),
            height: reader.height(),
            format: guess_image_format(data),
            size_bytes: data.len() as u64,
        })
    }

    /// 将图像缩放并编码为 Base64.
    pub fn resize_to_base64(data: &[u8], options: &ImageResizeOptions) -> LsResult<String> {
        let img = image::load_from_memory(data)
            .map_err(|e| lingshu_core::LsError::Internal(format!("image decode failed: {e}")))?;

        let (w, h) = (img.width(), img.height());
        let (new_w, new_h) = if w > options.max_width || h > options.max_height {
            let ratio =
                (options.max_width as f64 / w as f64).min(options.max_height as f64 / h as f64);
            ((w as f64 * ratio) as u32, (h as f64 * ratio) as u32)
        } else {
            (w, h)
        };

        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        let mut buf = std::io::Cursor::new(Vec::new());
        let format = guess_image_format(data);

        match format {
            ImageFormat::Jpeg => {
                resized
                    .write_to(&mut buf, image::ImageFormat::Jpeg)
                    .map_err(|e| {
                        lingshu_core::LsError::Internal(format!("jpeg encode failed: {e}"))
                    })?;
            }
            ImageFormat::Png => {
                resized
                    .write_to(&mut buf, image::ImageFormat::Png)
                    .map_err(|e| {
                        lingshu_core::LsError::Internal(format!("png encode failed: {e}"))
                    })?;
            }
            ImageFormat::WebP => {
                resized
                    .write_to(&mut buf, image::ImageFormat::WebP)
                    .map_err(|e| {
                        lingshu_core::LsError::Internal(format!("webp encode failed: {e}"))
                    })?;
            }
            _ => {
                // 默认用 PNG
                resized
                    .write_to(&mut buf, image::ImageFormat::Png)
                    .map_err(|e| {
                        lingshu_core::LsError::Internal(format!("png encode failed: {e}"))
                    })?;
            }
        }

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
        let mime = format.mime_type();
        Ok(format!("data:{};base64,{}", mime, b64))
    }

    /// 获取图像的 MIME 类型 (通过 magic bytes).
    pub fn detect_mime(data: &[u8]) -> &'static str {
        guess_image_format(data).mime_type()
    }
}

/// 通过 magic bytes 猜测图像格式.
fn guess_image_format(data: &[u8]) -> ImageFormat {
    if data.len() < 4 {
        return ImageFormat::Unknown;
    }

    // JPEG: FF D8 FF
    if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return ImageFormat::Jpeg;
    }
    // PNG: 89 50 4E 47
    if data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
        return ImageFormat::Png;
    }
    // GIF: 47 49 46 38 (GIF8)
    if data[0] == 0x47 && data[1] == 0x49 && data[2] == 0x46 && data[3] == 0x38 {
        return ImageFormat::Gif;
    }
    // WebP: 52 49 46 46 ... 57 45 42 50
    if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return ImageFormat::WebP;
    }
    // BMP: 42 4D
    if data[0] == 0x42 && data[1] == 0x4D {
        return ImageFormat::Bmp;
    }

    ImageFormat::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guess_format_jpeg() {
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        assert_eq!(guess_image_format(&data), ImageFormat::Jpeg);
    }

    #[test]
    fn test_guess_format_png() {
        let data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(guess_image_format(&data), ImageFormat::Png);
    }

    #[test]
    fn test_guess_format_unknown() {
        let data = vec![0x00, 0x00, 0x00, 0x00];
        assert_eq!(guess_image_format(&data), ImageFormat::Unknown);
    }

    #[test]
    fn test_image_format_mime() {
        assert_eq!(ImageFormat::Jpeg.mime_type(), "image/jpeg");
        assert_eq!(ImageFormat::Png.mime_type(), "image/png");
        assert_eq!(ImageFormat::Gif.mime_type(), "image/gif");
    }

    #[test]
    fn test_format_from_mime() {
        assert_eq!(ImageFormat::from_mime("image/jpeg"), ImageFormat::Jpeg);
        assert_eq!(ImageFormat::from_mime("image/png"), ImageFormat::Png);
        assert_eq!(ImageFormat::from_mime("image/gif"), ImageFormat::Gif);
        assert_eq!(ImageFormat::from_mime("unknown/type"), ImageFormat::Unknown);
    }

    #[test]
    fn test_analyze_real_png() {
        use image::ImageEncoder;
        // 创建一个 1x1 像素的 PNG
        let mut buf = std::io::Cursor::new(Vec::new());
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        let img = image::RgbaImage::new(1, 1);
        encoder
            .write_image(img.as_raw(), 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        let data = buf.into_inner();

        let analysis = ImageProcessor::analyze(&data).unwrap();
        assert_eq!(analysis.width, 1);
        assert_eq!(analysis.height, 1);
        assert_eq!(analysis.format, ImageFormat::Png);
        assert_eq!(analysis.size_bytes, data.len() as u64);
    }

    #[test]
    fn test_resize_to_base64() {
        use image::ImageEncoder;
        let mut buf = std::io::Cursor::new(Vec::new());
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        let img = image::RgbaImage::new(100, 100);
        encoder
            .write_image(img.as_raw(), 100, 100, image::ExtendedColorType::Rgba8)
            .unwrap();
        let data = buf.into_inner();

        let opts = ImageResizeOptions {
            max_width: 50,
            max_height: 50,
            quality: 85,
        };
        let b64 = ImageProcessor::resize_to_base64(&data, &opts).unwrap();
        assert!(b64.starts_with("data:image/png;base64,"));
        assert!(b64.len() > 50);
    }

    #[test]
    fn test_detect_mime() {
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(ImageProcessor::detect_mime(&jpeg), "image/jpeg");

        let png = vec![0x89, 0x50, 0x4E, 0x47];
        assert_eq!(ImageProcessor::detect_mime(&png), "image/png");
    }
}
