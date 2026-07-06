//! CodeUnderstandingPipeline — 代码理解流水线.
//!
//! 将代码分析各阶段串联为端到端流水线：
//! 扫描 → 图谱构建 → LLM 语义 Enrichment → 增量监听.
//!
//! ## 流水线阶段
//!
//! ```text
//! ┌────────┐   ┌──────────────┐   ┌─────────────┐   ┌────────────────┐
//! │  Scan  │ → │ GraphBuilder │ → │ LLM Enrich  │ → │  Watch & Diff  │
//! │(目录)  │   │(规则分析)   │   │(语义增强)   │   │ (增量更新)    │
//! └────────┘   └──────────────┘   └─────────────┘   └────────────────┘
//! ```

use std::sync::Arc;

use lingshu_code_analyzer::{
    AnalysisResult, ChangeCollector, EnrichmentQueue, EnrichmentTask, FileChangeEvent,
    FileChangeKind, FileEntry, FileScanner, GraphGenerator, LlmAnalyzer, ProjectSummary,
};
use lingshu_core::LsResult;
use lingshu_knowledge_graph::{Complexity, KnowledgeGraph};
use serde::{Deserialize, Serialize};
use tracing::info;

/// 流水线配置.
#[derive(Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// 项目根目录.
    pub project_root: String,
    /// 项目名称（用于图谱标识）.
    pub project_name: String,
    /// 项目描述.
    pub project_description: String,
    /// 是否启用语义分析（需要 LLM 后端）.
    pub enable_semantic_analysis: bool,
    /// 扫描最大文件数.
    pub max_files: usize,
    /// LLM enrichment 批次大小（0=使用 LLM 默认配置）.
    pub enrichment_batch_size: usize,
    /// 是否启用增量文件监听.
    pub enable_watch: bool,
    /// 轮询间隔（秒，仅轮询模式）.
    pub poll_interval_secs: u64,
    /// 增量扫描时间戳（只扫描此时间后修改的文件）.
    #[serde(skip)]
    pub modified_since: Option<std::time::SystemTime>,
    /// 扫描进度回调 (已扫描数, 总数).
    #[serde(skip)]
    pub progress_callback: Option<std::sync::Arc<dyn Fn(usize, usize) + Send + Sync>>,
}

impl std::fmt::Debug for PipelineConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineConfig")
            .field("project_root", &self.project_root)
            .field("project_name", &self.project_name)
            .field("project_description", &self.project_description)
            .field("enable_semantic_analysis", &self.enable_semantic_analysis)
            .field("max_files", &self.max_files)
            .field("enrichment_batch_size", &self.enrichment_batch_size)
            .field("enable_watch", &self.enable_watch)
            .field("poll_interval_secs", &self.poll_interval_secs)
            .field("modified_since", &self.modified_since)
            .field("progress_callback", &"<callback>")
            .finish()
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            project_root: String::new(),
            project_name: "unknown".into(),
            project_description: String::new(),
            enable_semantic_analysis: false,
            max_files: 10000,
            enrichment_batch_size: 0,
            enable_watch: false,
            poll_interval_secs: 5,
            modified_since: None,
            progress_callback: None,
        }
    }
}

/// 流水线报告.
#[derive(Clone, Serialize, Deserialize)]
pub struct PipelineReport {
    /// 扫描的文件数.
    pub files_scanned: usize,
    /// 图谱节点数.
    pub graph_nodes: usize,
    /// 图谱边数.
    pub graph_edges: usize,
    /// 耗时（毫秒）.
    pub duration_ms: u64,
    /// Enrichment 是否已提交.
    pub enrichment_submitted: bool,
    /// 提交 enrichment 的文件数.
    pub enrichment_files: usize,
    /// 是否已启动增量监听.
    pub watch_active: bool,
}

/// 增量变更处理结果.
#[derive(Clone, Serialize, Deserialize)]
pub struct IncrementalChange {
    pub file_path: String,
    pub change_kind: String,
    pub re_analyzed: bool,
}

pub struct CodeUnderstandingPipeline {
    scanner: FileScanner,
    config: PipelineConfig,
    /// 可选的 LLM 语义分析器（Hybrid 模式）.
    llm_analyzer: Option<Arc<LlmAnalyzer>>,
    /// 可选的变更收集器（增量监听用）.
    change_collector: Option<Arc<ChangeCollector>>,
}

impl CodeUnderstandingPipeline {
    /// 创建新的流水线.
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            scanner: FileScanner::new(),
            config,
            llm_analyzer: None,
            change_collector: None,
        }
    }

    /// 设置 LLM 分析器（启用 Hybrid 语义分析）.
    pub fn with_llm_analyzer(mut self, analyzer: Arc<LlmAnalyzer>) -> Self {
        self.llm_analyzer = Some(analyzer);
        self
    }

    /// 设置变更收集器（启用增量监听）.
    pub fn with_change_collector(mut self, collector: Arc<ChangeCollector>) -> Self {
        self.change_collector = Some(collector);
        self
    }

    /// 运行完整流水线：扫描 → 图谱构建 → 启动增量监听（可选）.
    ///
    /// 返回 (图谱, 报告, 可选的 enrichment 队列, 可选的变更收集器).
    pub async fn run(
        &self,
    ) -> LsResult<(
        KnowledgeGraph,
        PipelineReport,
        Option<Arc<EnrichmentQueue>>,
        Option<Arc<ChangeCollector>>,
    )> {
        let start = std::time::Instant::now();

        // 1. 扫描目录
        info!("Scanning project: {}", self.config.project_root);
        let project_root = std::path::Path::new(&self.config.project_root);
        let mut entries = self.scanner.scan(project_root)?;

        // 增量扫描：过滤掉 modified_since 之前未修改的文件
        if let Some(since) = self.config.modified_since {
            entries.retain(|e| {
                let path = std::path::Path::new(&e.path);
                path.metadata()
                    .and_then(|m| m.modified())
                    .map(|t| t >= since)
                    .unwrap_or(true)
            });
        }

        let files_scanned = entries.len().min(self.config.max_files);
        let total = entries.len();
        let file_entries: Vec<FileEntry> = entries
            .iter()
            .take(self.config.max_files)
            .cloned()
            .collect();
        info!("Scanned {files_scanned} files");

        // 进度回调
        if let Some(ref cb) = self.config.progress_callback {
            cb(files_scanned, total);
        }

        // 2. 构建基础分析结果（规则分析）
        let analyses: Vec<AnalysisResult> = file_entries
            .iter()
            .map(|e| AnalysisResult {
                file_path: e.path.clone(),
                summary: format!("{} file: {}", e.language, e.name),
                tags: vec![e.language.clone(), format!("{:?}", e.category)],
                complexity: Complexity::Simple,
                language_notes: Some(e.language.clone()),
                summaries: std::collections::HashMap::new(),
                file_summary: format!("{} source file: {}", e.language, e.name),
            })
            .collect();

        // 3. 构建项目概况
        let languages: Vec<String> = file_entries
            .iter()
            .map(|e| e.language.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let summary = ProjectSummary {
            name: self.config.project_name.clone(),
            description: self.config.project_description.clone(),
            languages,
            frameworks: vec![],
        };

        // 4. 构建初始知识图谱（基于规则分析）
        info!("Building knowledge graph");
        let generator = GraphGenerator::new();
        let graph = generator.generate(
            &self.config.project_name,
            "unknown",
            &file_entries,
            &analyses,
            &summary,
        );

        // 5. 可选：提交 LLM enrichment
        let (enrichment_queue, enrichment_submitted, enrichment_files) = if self
            .config
            .enable_semantic_analysis
        {
            if let Some(ref analyzer) = self.llm_analyzer {
                let batch_size = if self.config.enrichment_batch_size > 0 {
                    self.config.enrichment_batch_size
                } else {
                    analyzer.batch_size()
                };

                let queue = EnrichmentQueue::start(analyzer.clone(), batch_size);
                let file_count = file_entries.len();

                info!("Submitting {file_count} files for LLM enrichment");

                for entry in &file_entries {
                    let content = std::fs::read_to_string(&entry.path).unwrap_or_default();
                    let task = EnrichmentTask {
                        file_path: entry.path.clone(),
                        content,
                        language: entry.language.clone(),
                        callback: None,
                    };
                    queue.submit(task).await.map_err(|e| {
                        lingshu_core::LsError::Internal(format!("Failed to submit enrichment: {e}"))
                    })?;
                }

                (Some(queue), true, file_count)
            } else {
                (None, false, 0)
            }
        } else {
            (None, false, 0)
        };

        // 6. 可选：启动增量文件监听
        let watch_active = if self.config.enable_watch {
            self.start_watch().await?;
            true
        } else {
            false
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let report = PipelineReport {
            files_scanned,
            graph_nodes: graph.nodes.len(),
            graph_edges: graph.edges.len(),
            duration_ms,
            enrichment_submitted,
            enrichment_files,
            watch_active,
        };

        info!(
            "Pipeline complete: {files_scanned} files, {} nodes, {} edges in {duration_ms}ms",
            graph.nodes.len(),
            graph.edges.len()
        );

        let collector = self.change_collector.clone();
        Ok((graph, report, enrichment_queue, collector))
    }

    /// 启动增量文件监听（基于 project_root）.
    async fn start_watch(&self) -> LsResult<()> {
        let project_root = &self.config.project_root;
        let collector = self
            .change_collector
            .clone()
            .unwrap_or_else(|| Arc::new(ChangeCollector::new()));

        // 尝试使用原生通知，回退到轮询模式
        let observer: Box<dyn lingshu_code_analyzer::FileObserver> =
            match lingshu_code_analyzer::NotifyFileObserver::new() {
                Ok(notify_obs) => Box::new(notify_obs),
                Err(_) => {
                    tracing::warn!("native watcher unavailable, falling back to polling");
                    Box::new(lingshu_code_analyzer::PollingFileObserver::new())
                }
            };

        let cb: lingshu_code_analyzer::FileChangeCallback =
            Arc::new(move |event: FileChangeEvent| {
                collector.record(event);
            });
        observer.watch(project_root, cb).await?;
        info!(path = %project_root, "file watcher started");
        Ok(())
    }

    /// 处理增量变更 — 读取 ChangeCollector 中的累积事件，返回变更列表.
    ///
    /// 调用方应在合适间隔（如每秒）调用此方法获取增量变更。
    pub fn process_changes(&self) -> Vec<IncrementalChange> {
        let mut changes = Vec::new();

        if let Some(ref collector) = self.change_collector {
            let events = collector.drain();
            for event in &events {
                let change_kind = match event.kind {
                    FileChangeKind::Created => "created",
                    FileChangeKind::Modified => "modified",
                    FileChangeKind::Deleted => "deleted",
                    FileChangeKind::Renamed { .. } => "renamed",
                };

                changes.push(IncrementalChange {
                    file_path: event.path.to_string_lossy().to_string(),
                    change_kind: change_kind.to_string(),
                    re_analyzed: false, // 标记后续可重扫
                });

                info!(
                    path = %event.path.display(),
                    kind = %change_kind,
                    "file change detected"
                );
            }
        }

        changes
    }

    /// 仅扫描阶段（返回扫描结果）.
    pub fn scan_only(&self) -> LsResult<Vec<FileEntry>> {
        let project_root = std::path::Path::new(&self.config.project_root);
        self.scanner.scan(project_root)
    }

    /// 获取当前配置.
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// 获取 LLM 分析器引用.
    pub fn llm_analyzer(&self) -> Option<&Arc<LlmAnalyzer>> {
        self.llm_analyzer.as_ref()
    }

    /// 获取变更收集器引用.
    pub fn change_collector(&self) -> Option<&Arc<ChangeCollector>> {
        self.change_collector.as_ref()
    }
}

/// 创建默认的代码理解流水线（便捷函数）.
pub fn default_code_graph(project_root: &str, project_name: &str) -> CodeUnderstandingPipeline {
    let config = PipelineConfig {
        project_root: project_root.to_string(),
        project_name: project_name.to_string(),
        ..Default::default()
    };
    CodeUnderstandingPipeline::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_changes_empty() {
        let config = PipelineConfig {
            project_root: "/tmp".into(),
            project_name: "test".into(),
            ..Default::default()
        };
        let pipeline = CodeUnderstandingPipeline::new(config);
        let changes = pipeline.process_changes();
        assert!(changes.is_empty());
    }

    #[test]
    fn test_incremental_change_struct() {
        let change = IncrementalChange {
            file_path: "src/main.rs".into(),
            change_kind: "modified".into(),
            re_analyzed: false,
        };
        assert_eq!(change.file_path, "src/main.rs");
        assert_eq!(change.change_kind, "modified");
    }

    #[test]
    fn test_pipeline_config_default() {
        let config = PipelineConfig::default();
        assert_eq!(config.max_files, 10000);
        assert!(!config.enable_watch);
        assert_eq!(config.poll_interval_secs, 5);
    }
}
