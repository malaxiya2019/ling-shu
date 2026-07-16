//! TimelineWorkflow — 按时间线重建事件序列。
//!
//! # 工作流步骤
//!
//! ```text
//! 用户问题
//!    │
//!    ▼
//! Step 1: 提取实体（从问题中解析出实体名称和类型）
//!    │
//!    ▼
//! Step 2: Episode Search（按实体搜索相关事件）
//!    │
//!    ▼
//! Step 3: 时间排序（按时间戳升序排列）
//!    │
//!    ▼
//! Step 4: 去重合并（合并相同或高度相似的事件）
//!    │
//!    ▼
//! Step 5: 构建时间线（生成连续的 Timeline 结构）
//!    │
//!    ▼
//! Step 6: RoPE 时间衰减（对越久远的事件赋予越低置信度）
//! ```
//!
//! # 设计原则
//!
//! - **可观测**：每一步的输入输出都可以序列化
//! - **无智能**：不做因果推理，只做事实排序
//! - **可调试**：每个步骤可以独立验证

use chrono::{DateTime, Utc};
use lingshu_core::LsResult;
use lingshu_memory_episode::{
    EntityRef, Episode, EpisodeQuery, EpisodeRepository,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ─── Timeline 数据结构 ─────────────────────────────────

/// 时间线中的一个节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineNode {
    /// 原始事件 ID
    pub episode_id: String,
    /// 事件时间戳
    pub timestamp: DateTime<Utc>,
    /// 事件标题
    pub title: String,
    /// 事件摘要
    pub summary: String,
    /// 关联实体
    pub entities: Vec<EntityRef>,
    /// 标签
    pub tags: Vec<String>,
    /// 状态变化
    pub state_changes: Vec<String>,
    /// RoPE 时间衰减后的置信度 (0.0 ~ 1.0)
    pub confidence: f64,
}

/// 时间线 — 一个有序的事件序列。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    /// 时间线中的事件节点
    pub nodes: Vec<TimelineNode>,
    /// 时间跨度起始
    pub span_start: Option<DateTime<Utc>>,
    /// 时间跨度结束
    pub span_end: Option<DateTime<Utc>>,
    /// 涉及的所有实体
    pub involved_entities: Vec<EntityRef>,
    /// 涉及的所有标签
    pub involved_tags: Vec<String>,
    /// 事件总数
    pub total_events: usize,
}

/// TimelineWorkflow 执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineWorkflowResult {
    /// 构建的时间线
    pub timeline: Timeline,
    /// 工作流执行步骤记录
    pub steps: Vec<WorkflowStepRecord>,
    /// 执行耗时（毫秒）
    pub execution_time_ms: u64,
    /// 搜索时使用的查询
    pub query_used: String,
}

/// 工作流步骤记录（用于可观测性和调试）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepRecord {
    /// 步骤名称
    pub step: String,
    /// 步骤状态
    pub status: String,
    /// 步骤输出摘要
    pub summary: String,
    /// 耗时（毫秒）
    pub duration_ms: u64,
}

// ─── RoPE 时间衰减配置 ─────────────────────────────────

/// RoPE 时间衰减配置。
///
/// 受 Grok-1 的 RoPE（Rotary Position Embedding）启发，
/// 对时间线中越久远的事件赋予越低的置信度。
///
/// # 衰减公式
///
/// ```text
/// confidence = exp(-decay_rate * hours_ago)
/// ```
///
/// 默认 decay_rate = 0.001 每小时，约 50% / 30 天衰减。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoPEConfig {
    /// 指数衰减率（每小时）。默认 0.001
    pub decay_rate: f64,
    /// 最大衰减小时数。超过此值后置信度锁定为 min_confidence
    pub max_decay_hours: f64,
    /// 最小置信度下限（避免信息完全消失）
    pub min_confidence: f64,
    /// 是否启用衰减
    pub enabled: bool,
}

impl Default for RoPEConfig {
    fn default() -> Self {
        Self {
            decay_rate: 0.001,
            max_decay_hours: 8760.0, // 1 年
            min_confidence: 0.05,
            enabled: true,
        }
    }
}

impl RoPEConfig {
    /// 创建一个禁用了衰减的配置（所有事件置信度保持 1.0）。
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// 设置衰减率（每小时）。
    pub fn with_decay_rate(mut self, rate: f64) -> Self {
        self.decay_rate = rate;
        self
    }

    /// 设置最大衰减小时数。
    pub fn with_max_decay_hours(mut self, hours: f64) -> Self {
        self.max_decay_hours = hours;
        self
    }

    /// 设置最小置信度。
    pub fn with_min_confidence(mut self, min: f64) -> Self {
        self.min_confidence = min;
        self
    }
}

// ─── TimelineWorkflow ──────────────────────────────────

/// TimelineWorkflow — 按时间线重建事件序列。
///
/// 输入：自然语言查询（如"项目A为什么暂停？"）
/// 输出：结构化时间线
pub struct TimelineWorkflow {
    episode_store: Box<dyn EpisodeRepository>,
    rope_config: RoPEConfig,
}

impl TimelineWorkflow {
    /// 创建一个新的 TimelineWorkflow（使用默认 RoPE 配置）。
    pub fn new(episode_store: Box<dyn EpisodeRepository>) -> Self {
        Self {
            episode_store,
            rope_config: RoPEConfig::default(),
        }
    }

    /// 使用自定义 RoPE 配置创建 TimelineWorkflow。
    pub fn with_rope_config(mut self, config: RoPEConfig) -> Self {
        self.rope_config = config;
        self
    }

    /// 执行 Timeline 工作流。
    ///
    /// 完整流程：
    /// 1. 从问题中提取实体
    /// 2. 按实体搜索 Episode
    /// 3. 时间排序
    /// 4. 去重合并
    /// 5. 构建 Timeline
    /// 6. 应用 RoPE 时间衰减
    pub async fn execute(&self, query_text: &str) -> LsResult<TimelineWorkflowResult> {
        let start = std::time::Instant::now();
        let mut steps = Vec::new();

        // Step 1: 提取实体
        let step1_start = std::time::Instant::now();
        let entities = self.extract_entities(query_text);
        steps.push(WorkflowStepRecord {
            step: "extract_entities".into(),
            status: "completed".into(),
            summary: format!("提取到 {} 个实体", entities.len()),
            duration_ms: step1_start.elapsed().as_millis() as u64,
        });
        debug!(entities = ?entities, "step 1: entities extracted");

        // Step 2: Episode Search
        let step2_start = std::time::Instant::now();
        let episodes = self.search_episodes(&entities, query_text).await?;
        steps.push(WorkflowStepRecord {
            step: "episode_search".into(),
            status: "completed".into(),
            summary: format!("搜索到 {} 个相关事件", episodes.len()),
            duration_ms: step2_start.elapsed().as_millis() as u64,
        });
        debug!(count = episodes.len(), "step 2: episodes searched");

        if episodes.is_empty() {
            let elapsed = start.elapsed().as_millis() as u64;
            return Ok(TimelineWorkflowResult {
                timeline: Timeline {
                    nodes: Vec::new(),
                    span_start: None,
                    span_end: None,
                    involved_entities: entities.clone(),
                    involved_tags: Vec::new(),
                    total_events: 0,
                },
                steps,
                execution_time_ms: elapsed,
                query_used: query_text.to_string(),
            });
        }

        // Step 3: 时间排序
        let step3_start = std::time::Instant::now();
        let sorted = self.sort_episodes(episodes);
        steps.push(WorkflowStepRecord {
            step: "sort".into(),
            status: "completed".into(),
            summary: format!("排序完成，时间跨度从 {} 到 {}",
                sorted.first().map(|e| e.timestamp.to_rfc3339()).unwrap_or_default(),
                sorted.last().map(|e| e.timestamp.to_rfc3339()).unwrap_or_default()),
            duration_ms: step3_start.elapsed().as_millis() as u64,
        });

        // Step 4: 去重合并
        let step4_start = std::time::Instant::now();
        let deduped = self.deduplicate(sorted);
        steps.push(WorkflowStepRecord {
            step: "deduplicate".into(),
            status: "completed".into(),
            summary: format!("去重后剩余 {} 个事件", deduped.len()),
            duration_ms: step4_start.elapsed().as_millis() as u64,
        });

        // Step 5: 构建 Timeline
        let step5_start = std::time::Instant::now();
        let mut timeline = self.build_timeline(&deduped);
        steps.push(WorkflowStepRecord {
            step: "build_timeline".into(),
            status: "completed".into(),
            summary: format!("构建了 {} 个节点的时间线", timeline.nodes.len()),
            duration_ms: step5_start.elapsed().as_millis() as u64,
        });

        // Step 6: RoPE 时间衰减
        let step6_start = std::time::Instant::now();
        let decay_enabled = self.rope_config.enabled;
        self.apply_rope_decay(&mut timeline);
        let rope_summary = if self.rope_config.enabled {
            let avg_conf: f64 = if timeline.nodes.is_empty() {
                1.0
            } else {
                timeline.nodes.iter().map(|n| n.confidence).sum::<f64>() / timeline.nodes.len() as f64
            };
            format!("RoPE 衰减完成，平均置信度 {:.3}", avg_conf)
        } else {
            "RoPE 衰减已禁用".into()
        };
        steps.push(WorkflowStepRecord {
            step: "rope_decay".into(),
            status: "completed".into(),
            summary: rope_summary,
            duration_ms: step6_start.elapsed().as_millis() as u64,
        });

        let elapsed = start.elapsed().as_millis() as u64;

        info!(
            query = %query_text,
            events = timeline.total_events,
            time_ms = elapsed,
            rope_enabled = decay_enabled,
            "TimelineWorkflow completed"
        );

        Ok(TimelineWorkflowResult {
            timeline,
            steps,
            execution_time_ms: elapsed,
            query_used: query_text.to_string(),
        })
    }

    /// 应用 RoPE 时间衰减。
    ///
    /// 对每个节点按时间衰减置信度：`confidence = exp(-decay_rate * hours_ago)`
    /// 置信度锁定在 [min_confidence, 1.0] 范围内。
    fn apply_rope_decay(&self, timeline: &mut Timeline) {
        if !self.rope_config.enabled {
            return;
        }
        let now = Utc::now();
        for node in &mut timeline.nodes {
            let hours_ago = (now - node.timestamp).num_seconds() as f64 / 3600.0;
            let clamped_hours = hours_ago.min(self.rope_config.max_decay_hours);
            let decayed = (-self.rope_config.decay_rate * clamped_hours).exp();
            node.confidence = decayed.max(self.rope_config.min_confidence);
        }
    }

    // ─── 内部步骤方法 ──────────────────────────────────

    /// Step 1: 从自然语言查询中提取实体。
    ///
    /// 当前实现使用简单的关键词模式匹配。
    /// 后续可以替换为 LLM-based 实体提取。
    fn extract_entities(&self, query: &str) -> Vec<EntityRef> {
        let mut entities = Vec::new();
        let lower = query.to_lowercase();

        // 常见项目名称模式：X项目、项目X
        if let Some(pos) = lower.find("项目") {
            // 尝试提取项目名：项目后面的第一个ASCII字母数字序列
            let after = &lower[pos + "项目".len()..];
            if let Some(m) = after.chars().next().and_then(|c| {
                if c.is_ascii_alphanumeric() {
                    let name: String = after.chars().take_while(|ch| ch.is_ascii_alphanumeric()).collect();
                    Some(name)
                } else {
                    None
                }
            }) {
                if !m.is_empty() {
                    entities.push(EntityRef::new("project", m));
                }
            }

            // 也尝试提取项目X作为完整实体名
            let after_full = &query[pos..];
            let project_name: String = after_full.chars().take_while(|ch| !ch.is_whitespace() && *ch != '为' && *ch != '的' && *ch != '是' && *ch != '？' && *ch != '?' && *ch != '，' && *ch != '。' && *ch != '！' && *ch != '了').collect();
            if project_name.len() > 2 {
                entities.push(EntityRef::new("project", project_name));
            }
        }

        // 人员模式：X说、X认为
        let chars: Vec<char> = lower.chars().collect();
        for i in 0..chars.len().saturating_sub(1) {
            if (chars[i] == '说' || (i + 1 < chars.len() && chars[i+1] == '说')) && i > 0 {
                let name_start = if chars[i] == '说' { i.saturating_sub(3) } else { i.saturating_sub(1) };
                let name: String = chars[name_start..i].iter().collect();
                let name = name.trim();
                if !name.is_empty() && name.len() <= 6 {
                    entities.push(EntityRef::new("person", name));
                }
            }
        }

        // 如果没提取到实体，用关键词搜索
        if entities.is_empty() {
            // 去停用词，提取有意义的中文/英文关键词
            let words: Vec<String> = lower
                .split(|c: char| c.is_whitespace() || c == '?' || c == '？' || c == '！' || c == '。' || c == '，')
                .filter(|w| !w.is_empty())
                .filter(|w| w.len() > 1 && !is_stopword(w))
                .take(3)
                .map(|w| w.to_string())
                .collect();
            for word in words {
                entities.push(EntityRef::new("keyword", word));
            }
        }

        entities
    }

    /// Step 2: 搜索 Episode。
    async fn search_episodes(
        &self,
        entities: &[EntityRef],
        query_text: &str,
    ) -> LsResult<Vec<Episode>> {
        // Step 1: 优先按实体搜索
        if !entities.is_empty() {
            let mut entity_query = EpisodeQuery::default().with_limit(500);
            for entity in entities {
                entity_query = entity_query.with_entity(entity.clone());
            }
            let results = self.episode_store.query(entity_query).await?;
            if !results.is_empty() {
                return Ok(results);
            }
        }

        // Step 2: 实体搜索无结果，兜底到文本搜索
        if !query_text.is_empty() {
            let text_query = EpisodeQuery::default()
                .with_limit(500)
                .with_search(query_text);
            let results = self.episode_store.query(text_query).await?;
            return Ok(results);
        }

        Ok(Vec::new())
    }

    /// Step 3: 按时间排序（升序）。
    fn sort_episodes(&self, mut episodes: Vec<Episode>) -> Vec<Episode> {
        episodes.sort_by_key(|e| e.timestamp);
        episodes
    }

    /// Step 4: 去重合并（简单的时间窗口去重）。
    fn deduplicate(&self, episodes: Vec<Episode>) -> Vec<Episode> {
        if episodes.is_empty() {
            return episodes;
        }

        let mut result = Vec::with_capacity(episodes.len());
        let mut seen: std::collections::HashSet<(String, i64)> =
            std::collections::HashSet::new();

        for ep in episodes {
            let time_bucket = ep.timestamp.timestamp() / 300; // 5分钟窗口
            let key = (ep.title.clone(), time_bucket);
            if seen.insert(key) {
                result.push(ep);
            }
        }

        result
    }

    /// Step 5: 构建 Timeline 结构。
    ///
    /// 每个节点的初始置信度为 1.0，后续由 RoPE 衰减处理。
    fn build_timeline(&self, episodes: &[Episode]) -> Timeline {
        if episodes.is_empty() {
            return Timeline {
                nodes: Vec::new(),
                span_start: None,
                span_end: None,
                involved_entities: Vec::new(),
                involved_tags: Vec::new(),
                total_events: 0,
            };
        }

        let nodes: Vec<TimelineNode> = episodes
            .iter()
            .map(|ep| TimelineNode {
                episode_id: ep.id.to_string(),
                timestamp: ep.timestamp,
                title: ep.title.clone(),
                summary: ep.summary.clone(),
                entities: ep.entities.clone(),
                tags: ep.tags.clone(),
                state_changes: ep
                    .state_changes
                    .iter()
                    .map(|sc| format!("{}: {} → {}", sc.change_type, sc.from.as_deref().unwrap_or("?"), sc.to))
                    .collect(),
                confidence: 1.0,
            })
            .collect();

        let mut all_entities: Vec<EntityRef> = Vec::new();
        let mut all_tags: Vec<String> = Vec::new();
        let mut entity_set = std::collections::HashSet::new();
        let mut tag_set = std::collections::HashSet::new();

        for ep in episodes {
            for entity in &ep.entities {
                let key = format!("{}:{}", entity.kind, entity.name);
                if entity_set.insert(key) {
                    all_entities.push(entity.clone());
                }
            }
            for tag in &ep.tags {
                if tag_set.insert(tag.clone()) {
                    all_tags.push(tag.clone());
                }
            }
        }

        Timeline {
            span_start: Some(episodes.first().unwrap().timestamp),
            span_end: Some(episodes.last().unwrap().timestamp),
            involved_entities: all_entities,
            involved_tags: all_tags,
            total_events: nodes.len(),
            nodes,
        }
    }
}

// ─── 工具函数 ──────────────────────────────────────────

fn is_stopword(word: &str) -> bool {
    let stopwords = [
        "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一",
        "一个", "上", "也", "很", "到", "说", "要", "去", "你", "会", "着",
        "没有", "看", "好", "自己", "这", "他", "她", "它", "们",
        "为什么", "怎么", "如何", "哪些", "什么", "哪个",
        "the", "a", "an", "is", "are", "was", "were", "be", "been",
        "i", "you", "he", "she", "it", "we", "they",
        "do", "does", "did", "have", "has", "had",
    ];
    stopwords.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_memory_episode::{InMemoryEpisodeStore, StateChange};
    use chrono::Duration;

    async fn setup_store_with_episodes() -> InMemoryEpisodeStore {
        let store = InMemoryEpisodeStore::new();

        // 项目A 的事件序列
        let episodes = vec![
            Episode::new("启动项目A", "团队决定启动项目A的开发", Utc::now() - Duration::days(60))
                .with_entity(EntityRef::new("project", "项目A"))
                .with_entity(EntityRef::new("person", "张三"))
                .with_tag("launch"),
            Episode::new("项目A完成需求评审", "需求文档通过评审", Utc::now() - Duration::days(50))
                .with_entity(EntityRef::new("project", "项目A"))
                .with_tag("milestone"),
            Episode::new("供应商退出", "核心供应商突然退出合作", Utc::now() - Duration::days(20))
                .with_entity(EntityRef::new("project", "项目A"))
                .with_entity(EntityRef::new("organization", "供应商X"))
                .with_state_change(StateChange::new(
                    EntityRef::new("project", "项目A"), "status", Some("active".to_string()), "blocked",
                ))
                .with_tag("risk"),
            Episode::new("暂停项目A", "因供应商问题暂停项目A", Utc::now() - Duration::days(15))
                .with_entity(EntityRef::new("project", "项目A"))
                .with_state_change(StateChange::new(
                    EntityRef::new("project", "项目A"), "status", Some("blocked".to_string()), "paused",
                ))
                .with_tag("decision"),
        ];

        for ep in episodes {
            let _ = store.store(ep).await;
        }

        store
    }

    #[tokio::test]
    async fn test_timeline_workflow_basic() {
        let store = setup_store_with_episodes().await;
        let workflow = TimelineWorkflow::new(Box::new(store));

        let result = workflow.execute("项目A为什么暂停？").await.unwrap();

        assert_eq!(result.timeline.total_events, 4);
        assert!(!result.steps.is_empty());
        assert!(result.execution_time_ms >= 0);

        // 时间线应该是升序的
        let timestamps: Vec<_> = result.timeline.nodes.iter().map(|n| n.timestamp).collect();
        let mut sorted = timestamps.clone();
        sorted.sort();
        assert_eq!(timestamps, sorted);
    }

    #[tokio::test]
    async fn test_empty_result() {
        let store = InMemoryEpisodeStore::new();
        let workflow = TimelineWorkflow::new(Box::new(store));

        let result = workflow.execute("不存在的项目").await.unwrap();
        assert_eq!(result.timeline.total_events, 0);
        assert!(result.execution_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_timeline_contains_state_changes() {
        let store = setup_store_with_episodes().await;
        let workflow = TimelineWorkflow::new(Box::new(store));

        let result = workflow.execute("项目A").await.unwrap();

        // 至少有一个节点包含状态变化
        let has_state_change = result.timeline.nodes.iter().any(|n| !n.state_changes.is_empty());
        assert!(has_state_change, "time should contain state changes");
    }

    #[test]
    fn test_extract_entities_project() {
        let workflow = TimelineWorkflow::new(Box::new(InMemoryEpisodeStore::new()));
        let entities = workflow.extract_entities("项目A为什么暂停？");
        let has_project = entities.iter().any(|e| e.kind == "project" && e.name == "项目A");
        assert!(has_project, "should extract project entity");
    }

    #[test]
    fn test_extract_entities_person() {
        let workflow = TimelineWorkflow::new(Box::new(InMemoryEpisodeStore::new()));
        let entities = workflow.extract_entities("张三说项目进展顺利");
        let has_person = entities.iter().any(|e| e.kind == "person" && e.name == "张三");
        assert!(has_person, "should extract person entity");
    }

    #[test]
    fn test_deduplicate_exact_duplicates() {
        let workflow = TimelineWorkflow::new(Box::new(InMemoryEpisodeStore::new()));
        let ep = Episode::new("相同事件", "测试", Utc::now());
        let episodes = vec![ep.clone(), ep.clone()];
        let result = workflow.deduplicate(episodes);
        assert_eq!(result.len(), 1, "exact duplicates should be deduped");
    }

    // ─── RoPE 时间衰减测试 ─────────────────────────────

    #[test]
    fn test_rope_config_default() {
        let cfg = RoPEConfig::default();
        assert!((cfg.decay_rate - 0.001).abs() < 1e-10);
        assert!((cfg.max_decay_hours - 8760.0).abs() < 1e-10);
        assert!((cfg.min_confidence - 0.05).abs() < 1e-10);
        assert!(cfg.enabled);
    }

    #[test]
    fn test_rope_config_disabled() {
        let cfg = RoPEConfig::disabled();
        assert!(!cfg.enabled);
    }

    #[test]
    fn test_apply_rope_decay_older_lower_confidence() {
        let now = Utc::now();
        let old_node = TimelineNode {
            episode_id: "old".into(),
            timestamp: now - Duration::days(60), // 60天前
            title: "老事件".into(),
            summary: "".into(),
            entities: vec![],
            tags: vec![],
            state_changes: vec![],
            confidence: 1.0,
        };
        let recent_node = TimelineNode {
            episode_id: "recent".into(),
            timestamp: now - Duration::hours(1), // 1小时前
            title: "新事件".into(),
            summary: "".into(),
            entities: vec![],
            tags: vec![],
            state_changes: vec![],
            confidence: 1.0,
        };

        let mut timeline = Timeline {
            nodes: vec![old_node, recent_node],
            span_start: None,
            span_end: None,
            involved_entities: vec![],
            involved_tags: vec![],
            total_events: 2,
        };

        let workflow = TimelineWorkflow::new(Box::new(InMemoryEpisodeStore::new()));
        workflow.apply_rope_decay(&mut timeline);

        // 旧事件置信度应该低于新事件
        assert!(
            timeline.nodes[0].confidence < timeline.nodes[1].confidence,
            "old event ({}) should have lower confidence than recent event ({})",
            timeline.nodes[0].confidence,
            timeline.nodes[1].confidence,
        );

        // 置信度应该在 [min_confidence, 1.0] 范围内
        for node in &timeline.nodes {
            assert!(
                node.confidence >= workflow.rope_config.min_confidence,
                "confidence {} should be >= min_confidence {}",
                node.confidence,
                workflow.rope_config.min_confidence,
            );
            assert!(node.confidence <= 1.0);
        }
    }

    #[test]
    fn test_rope_decay_disabled_maintains_full_confidence() {
        let now = Utc::now();
        let node = TimelineNode {
            episode_id: "test".into(),
            timestamp: now - Duration::days(365), // 1年前
            title: "旧事件".into(),
            summary: "".into(),
            entities: vec![],
            tags: vec![],
            state_changes: vec![],
            confidence: 1.0,
        };

        let mut timeline = Timeline {
            nodes: vec![node],
            span_start: None,
            span_end: None,
            involved_entities: vec![],
            involved_tags: vec![],
            total_events: 1,
        };

        let workflow = TimelineWorkflow::new(Box::new(InMemoryEpisodeStore::new()))
            .with_rope_config(RoPEConfig::disabled());
        workflow.apply_rope_decay(&mut timeline);

        // 禁用后置信度保持 1.0
        assert!(
            (timeline.nodes[0].confidence - 1.0).abs() < 1e-10,
            "disabled RoPE should keep confidence at 1.0, got {}",
            timeline.nodes[0].confidence,
        );
    }

    #[test]
    fn test_rope_decay_very_old_event_floor_at_min_confidence() {
        let now = Utc::now();
        let node = TimelineNode {
            episode_id: "ancient".into(),
            timestamp: now - Duration::days(365 * 10), // 10年前
            title: "远古事件".into(),
            summary: "".into(),
            entities: vec![],
            tags: vec![],
            state_changes: vec![],
            confidence: 1.0,
        };

        let mut timeline = Timeline {
            nodes: vec![node],
            span_start: None,
            span_end: None,
            involved_entities: vec![],
            involved_tags: vec![],
            total_events: 1,
        };

        let workflow = TimelineWorkflow::new(Box::new(InMemoryEpisodeStore::new()))
            .with_rope_config(RoPEConfig::default().with_min_confidence(0.1));
        workflow.apply_rope_decay(&mut timeline);

        // 10年前的事件，衰减后应该触底
        assert!(
            (timeline.nodes[0].confidence - 0.1).abs() < 1e-10,
            "ancient event confidence should floor at min_confidence 0.1, got {}",
            timeline.nodes[0].confidence,
        );
    }

    #[tokio::test]
    async fn test_timeline_workflow_with_rope_step() {
        let store = setup_store_with_episodes().await;
        let workflow = TimelineWorkflow::new(Box::new(store));

        let result = workflow.execute("项目A为什么暂停？").await.unwrap();

        // 应该有6个步骤（含新加的 rope_decay）
        assert_eq!(result.steps.len(), 6);

        // 最后一步应该是 rope_decay
        let last_step = result.steps.last().unwrap();
        assert_eq!(last_step.step, "rope_decay");
        assert_eq!(last_step.status, "completed");

        // 所有节点都应该有置信度
        for node in &result.timeline.nodes {
            assert!(
                node.confidence > 0.0 && node.confidence <= 1.0,
                "confidence should be in (0, 1], got {}",
                node.confidence,
            );
        }
    }

    #[tokio::test]
    async fn test_rope_decay_disabled_in_full_workflow() {
        let store = setup_store_with_episodes().await;
        let workflow = TimelineWorkflow::new(Box::new(store))
            .with_rope_config(RoPEConfig::disabled());

        let result = workflow.execute("项目A为什么暂停？").await.unwrap();

        // 禁用时所有节点置信度应为 1.0
        for node in &result.timeline.nodes {
            assert!(
                (node.confidence - 1.0).abs() < 1e-10,
                "disabled RoPE should keep confidence at 1.0, got {}",
                node.confidence,
            );
        }
    }
}
