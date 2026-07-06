//! 多模态 RAG 模块 — 跨模态检索增强生成.

use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 多模态文档.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalDocument {
    /// 文档 ID
    pub id: LsId,
    /// 文档标题
    pub title: String,
    /// 文本内容
    pub text: String,
    /// 关联的图像 (Base64 data URL 或 文件路径)
    pub images: Vec<String>,
    /// 关联的音频 (Base64 data URL 或 文件路径)
    pub audio: Vec<String>,
    /// 文档元数据
    pub metadata: HashMap<String, String>,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 多模态检索结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalSearchResult {
    pub documents: Vec<MultimodalDocument>,
    pub total: usize,
    pub query: String,
}

/// 多模态检索器 trait.
#[async_trait::async_trait]
pub trait MultimodalRetriever: Send + Sync + 'static {
    /// 索引一个多模态文档.
    async fn index(&self, ctx: &LsContext, doc: MultimodalDocument) -> LsResult<()>;

    /// 搜索相关文档.
    async fn search(
        &self,
        ctx: &LsContext,
        query: &str,
        limit: usize,
    ) -> LsResult<MultimodalSearchResult>;

    /// 删除文档.
    async fn delete(&self, ctx: &LsContext, doc_id: &LsId) -> LsResult<()>;

    /// 列出所有文档.
    async fn list(&self, ctx: &LsContext) -> LsResult<Vec<MultimodalDocument>>;
}

/// 简单的内存多模态 RAG 实现.
pub struct MultimodalRag {
    documents: tokio::sync::RwLock<HashMap<String, MultimodalDocument>>,
}

impl MultimodalRag {
    pub fn new() -> Self {
        Self {
            documents: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 创建消息内容，将文本和图像组合为 ContentPart 列表.
    pub fn build_multimodal_content(
        text: &str,
        images: &[String],
    ) -> Vec<lingshu_traits::llm::ContentPart> {
        let mut parts = Vec::new();

        if !text.is_empty() {
            parts.push(lingshu_traits::llm::ContentPart::text(text));
        }

        for img_url in images {
            parts.push(lingshu_traits::llm::ContentPart::image_url(
                lingshu_traits::llm::ImageUrl::new(img_url),
            ));
        }

        parts
    }

    /// 将文件分析结果转换为多模态消息内容.
    pub fn file_to_message_content(
        text: &str,
        file_data: &[u8],
        mime_type: &str,
    ) -> Vec<lingshu_traits::llm::ContentPart> {
        let mut parts = Vec::new();

        if !text.is_empty() {
            parts.push(lingshu_traits::llm::ContentPart::text(text));
        }

        if mime_type.starts_with("image/") {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(file_data);
            let data_url = format!("data:{};base64,{}", mime_type, b64);
            parts.push(lingshu_traits::llm::ContentPart::image_url(
                lingshu_traits::llm::ImageUrl::new(data_url),
            ));
        }

        parts
    }
}

#[async_trait::async_trait]
impl MultimodalRetriever for MultimodalRag {
    async fn index(&self, _ctx: &LsContext, doc: MultimodalDocument) -> LsResult<()> {
        let mut docs = self.documents.write().await;
        docs.insert(doc.id.to_string(), doc);
        Ok(())
    }

    async fn search(
        &self,
        _ctx: &LsContext,
        query: &str,
        limit: usize,
    ) -> LsResult<MultimodalSearchResult> {
        let docs = self.documents.read().await;
        let query_lower = query.to_lowercase();

        // 简单的关键词匹配
        let mut matched: Vec<MultimodalDocument> = docs
            .values()
            .filter(|doc| {
                doc.text.to_lowercase().contains(&query_lower)
                    || doc.title.to_lowercase().contains(&query_lower)
                    || doc
                        .metadata
                        .values()
                        .any(|v| v.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect();

        matched.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        matched.truncate(limit);

        Ok(MultimodalSearchResult {
            total: matched.len(),
            documents: matched,
            query: query.to_string(),
        })
    }

    async fn delete(&self, _ctx: &LsContext, doc_id: &LsId) -> LsResult<()> {
        let mut docs = self.documents.write().await;
        docs.remove(&doc_id.to_string());
        Ok(())
    }

    async fn list(&self, _ctx: &LsContext) -> LsResult<Vec<MultimodalDocument>> {
        let docs = self.documents.read().await;
        let mut items: Vec<MultimodalDocument> = docs.values().cloned().collect();
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(items)
    }
}

impl Default for MultimodalRag {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_multimodal_content() {
        let parts = MultimodalRag::build_multimodal_content(
            "Hello",
            &["http://example.com/img.png".into()],
        );
        assert_eq!(parts.len(), 2);
        assert!(matches!(
            parts[0],
            lingshu_traits::llm::ContentPart::Text { .. }
        ));
        assert!(matches!(
            parts[1],
            lingshu_traits::llm::ContentPart::ImageUrl { .. }
        ));
    }

    #[test]
    fn test_build_multimodal_content_no_images() {
        let parts = MultimodalRag::build_multimodal_content("Just text", &[]);
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn test_file_to_message_content_image() {
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD.encode(b"fake image data");
        let parts = MultimodalRag::file_to_message_content(
            "What's in this image?",
            data.as_bytes(),
            "image/png",
        );
        assert_eq!(parts.len(), 2);
        assert!(parts
            .iter()
            .any(|p| matches!(p, lingshu_traits::llm::ContentPart::ImageUrl { .. })));
    }

    #[tokio::test]
    async fn test_rag_index_search() {
        let rag = MultimodalRag::new();
        let ctx = LsContext::with_session(LsId::new());

        let doc = MultimodalDocument {
            id: LsId::new(),
            title: "Cat Photo".into(),
            text: "A cute cat sitting on a mat".into(),
            images: vec!["http://example.com/cat.png".into()],
            audio: vec![],
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        rag.index(&ctx, doc).await.unwrap();

        let result = rag.search(&ctx, "cat", 10).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.documents[0].title, "Cat Photo");

        let result = rag.search(&ctx, "dog", 10).await.unwrap();
        assert_eq!(result.total, 0);
    }

    #[tokio::test]
    async fn test_rag_delete() {
        let rag = MultimodalRag::new();
        let ctx = LsContext::with_session(LsId::new());

        let id = LsId::new();
        let doc = MultimodalDocument {
            id: id.clone(),
            title: "Test".into(),
            text: "test content".into(),
            images: vec![],
            audio: vec![],
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };
        rag.index(&ctx, doc).await.unwrap();
        rag.delete(&ctx, &id).await.unwrap();

        let result = rag.list(&ctx).await.unwrap();
        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    async fn test_rag_list() {
        let rag = MultimodalRag::new();
        let ctx = LsContext::with_session(LsId::new());

        for i in 0..3 {
            let doc = MultimodalDocument {
                id: LsId::new(),
                title: format!("Doc {}", i),
                text: format!("content {}", i),
                images: vec![],
                audio: vec![],
                metadata: HashMap::new(),
                created_at: chrono::Utc::now(),
            };
            rag.index(&ctx, doc).await.unwrap();
        }

        let result = rag.list(&ctx).await.unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_image_url_from_base64() {
        let url = lingshu_traits::llm::ImageUrl::from_base64("image/png", "iVBORw0KGgo");
        assert!(url.url.starts_with("data:image/png;base64,"));
    }
}
