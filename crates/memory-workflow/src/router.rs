//! MemoryRouter — 概率化软路由（MoE 风格）。
//!
//! 受 Grok-1 MoE Router 启发，将 Lingshu 的硬路由升级为带权重的软路由：
//!
//! - 每个问题不再只路由到单一 workflow（Episode / Semantic / None）
//! - 而是对所有 workflow 计算一个概率分布（权重）
//! - 上层可并行执行多个 workflow 并按权重合并结果
//!
//! # 架构
//!
//! ```text
//! 用户问题
//!     │
//!     ▼
//! 规则预分类器  ──→  基础分类权重
//!     │
//!     ▼
//! Softmax 归一化  ──→  WeightedRoutes
//!     │
//!  ┌──┴──┐
//!  │     │
//! 并行执行  按权重合并
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 路由权重 — 每个 workflow 的选用概率 (0.0 ~ 1.0)。
pub type RouteWeights = HashMap<String, f64>;

/// ProbabilisticRouter — MoE 风格的软路由。
///
/// 核心思路：
/// - 所有 workflow 共享一个权重分布
/// - 权重通过规则基分类 + Softmax 归一化得到
/// - 支持自定义加权规则
///
/// # Grok-1 映射
///
/// | Grok-1 MoE | Lingshu ProbabilisticRouter |
/// |-----------|-----------------------------|
/// | Router 网络 → softmax → top-k experts | 规则分类 → softmax → top-k workflows |
/// | Expert weights (可学习) | 规则权重（可自定义） |
/// | Top-k routing | 阈值过滤（可配置） |
pub struct ProbabilisticRouter {
    /// 基础权重（分类规则的输出）
    base_weights: HashMap<String, f64>,
    /// 自定义加权规则
    custom_rules: Vec<Box<dyn Fn(&str) -> Option<RouteWeights> + Send + Sync>>,
    /// 温度参数（控制 softmax 的锐度，>1 更平滑，<1 更锐利）
    temperature: f64,
    /// 路由阈值（低于此权重的 workflow 被过滤）
    threshold: f64,
    /// 是否启用 top-k 过滤（只保留权重最高的 k 个）
    top_k: Option<usize>,
}

impl Default for ProbabilisticRouter {
    fn default() -> Self {
        Self {
            base_weights: HashMap::new(),
            custom_rules: Vec::new(),
            temperature: 1.0,
            threshold: 0.05,
            top_k: Some(3),
        }
    }
}

impl ProbabilisticRouter {
    /// 创建一个新的概率路由器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 softmax 温度。
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.max(0.1);
        self
    }

    /// 设置路由阈值。
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// 设置 top-k 过滤。
    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = Some(k);
        self
    }

    /// 禁用 top-k 过滤。
    pub fn without_top_k(mut self) -> Self {
        self.top_k = None;
        self
    }

    /// 添加自定义路由规则。
    pub fn add_rule(&mut self, rule: Box<dyn Fn(&str) -> Option<RouteWeights> + Send + Sync>) {
        self.custom_rules.push(rule);
    }

    /// 设置指定 workflow 的基础权重。
    pub fn set_base_weight(&mut self, name: &str, weight: f64) {
        self.base_weights.insert(name.to_string(), weight.clamp(0.0, 1.0));
    }

    /// 批量设置基础权重。
    pub fn set_base_weights(&mut self, weights: RouteWeights) {
        for (name, weight) in weights {
            self.base_weights.insert(name, weight.clamp(0.0, 1.0));
        }
    }

    /// 对问题执行路由，返回各 workflow 的权重分布。
    pub fn route(&self, question: &str) -> RouteWeights {
        // 1. 先检查自定义规则
        for rule in &self.custom_rules {
            if let Some(weights) = rule(question) {
                return self.normalize(weights);
            }
        }

        // 2. 规则基预分类 → 原始分数
        let mut raw: HashMap<String, f64> = HashMap::new();
        raw.insert("conversation".to_string(), 0.0);
        raw.insert("timeline".to_string(), 0.0);
        raw.insert("semantic".to_string(), 0.0);
        raw.insert("reflection".to_string(), 0.0);

        self.apply_default_rules(question, &mut raw);

        // 3. 叠加基础权重
        for (name, weight) in &self.base_weights {
            *raw.entry(name.clone()).or_insert(0.0) += weight;
        }

        // 4. 归一化
        self.normalize(raw)
    }

    /// 判断是否需要走 Memory Pipeline。
    pub fn needs_memory(&self, question: &str) -> bool {
        let weights = self.route(question);
        // timeline + semantic > threshold 则认为需要记忆
        // conversation 权重高表明不需要记忆（问候、翻译等）
        let conversation = *weights.get("conversation").unwrap_or(&0.0);
        let memory_weight = *weights.get("timeline").unwrap_or(&0.0)
            + *weights.get("semantic").unwrap_or(&0.0);
        // conversation 是主导时不需要记忆，否则检查是否有足够记忆信号
        let is_conversation_dominant = conversation > memory_weight && conversation > 0.3;
        !is_conversation_dominant && memory_weight > self.threshold
    }

    /// 获取所有启用 workflow 的权重列表（按权重降序）。
    pub fn sorted_weights(&self, question: &str) -> Vec<(String, f64)> {
        let mut weights: Vec<(String, f64)> = self.route(question).into_iter().collect();
        weights.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        weights
    }

    /// 获取 top-k 个 workflow 及其权重。
    pub fn top_k_workflows(&self, question: &str) -> Vec<(String, f64)> {
        let sorted = self.sorted_weights(question);
        match self.top_k {
            Some(k) => sorted.into_iter().take(k).collect(),
            None => sorted,
        }
    }

    // ─── 内部方法 ──────────────────────────────────────

    /// Softmax 归一化。
    fn normalize(&self, weights: RouteWeights) -> RouteWeights {
        if weights.is_empty() {
            return weights;
        }

        // 应用温度
        let tempered: HashMap<String, f64> = weights
            .into_iter()
            .map(|(k, v)| (k, v / self.temperature))
            .collect();

        let max_val = tempered
            .values()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);

        let exp_sum: f64 = tempered
            .values()
            .map(|v| (v - max_val).exp())
            .sum();

        if exp_sum == 0.0 {
            return tempered;
        }

        let mut normalized: RouteWeights = tempered
            .into_iter()
            .map(|(k, v)| {
                let prob = ((v - max_val).exp()) / exp_sum;
                (k, prob)
            })
            .collect();

        // 阈值过滤
        normalized.retain(|_, v| *v >= self.threshold);

        normalized
    }

    /// 默认分类规则。
    fn apply_default_rules(&self, question: &str, raw: &mut RouteWeights) {
        let lower = question.to_lowercase().trim().to_string();

        // ── 不需要记忆的信号 ────────────────────────
        let no_memory_signals = [
            // 纯数字运算
            lower.chars().all(|c| c.is_ascii_digit() || "+-*/() ".contains(c)),
            // 问候
            ["你好", "您好", "hi", "hello", "hey", "早上好", "下午好", "晚上好",
             "good morning", "good afternoon", "good evening"]
                .iter().any(|g| lower == *g),
            // 天气
            lower.contains("天气") || lower.contains("weather"),
            // 翻译
            lower.starts_with("翻译") || lower.contains("translate"),
            // 时间
            ["现在几点", "今天几号", "当前时间", "现在时间", "几点了"]
                .iter().any(|k| lower.contains(k)),
        ];

        if no_memory_signals.iter().any(|&x| x) {
            *raw.get_mut("conversation").unwrap_or(&mut 0.0) += 1.0;
            return;
        }

        // ── Episode/Timeline 信号 ──────────────────
        let timeline_signals: f64 = [
            "项目", "之前", "去年", "上个月", "上周", "昨天",
            "历史", "曾经", "当时", "那时候", "以前",
            "暂停", "停止", "启动", "开始", "完成", "结束",
            "为什么", "原因", "导致", "因为", "所以",
            "谁", "什么时候", "怎么", "如何",
            "时间线", "时间轴", "历程", "过程", "经过",
        ].iter().filter(|k| lower.contains(*k)).count() as f64;

        if timeline_signals >= 3.0 {
            *raw.get_mut("timeline").unwrap_or(&mut 0.0) += 1.0;
            *raw.get_mut("semantic").unwrap_or(&mut 0.0) += 0.3;
        } else if timeline_signals >= 1.0 {
            *raw.get_mut("timeline").unwrap_or(&mut 0.0) += 0.7;
            *raw.get_mut("semantic").unwrap_or(&mut 0.0) += 0.3;
        }

        // ── Semantic 信号 ──────────────────────────
        let semantic_signals: f64 = [
            "是什么", "什么是", "意思是", "概念", "定义",
            "知道", "了解", "熟悉", "认识",
            "提到", "说过", "聊过", "讨论",
        ].iter().filter(|k| lower.contains(*k)).count() as f64;

        if semantic_signals >= 1.0 {
            *raw.get_mut("semantic").unwrap_or(&mut 0.0) += 0.8;
            *raw.get_mut("timeline").unwrap_or(&mut 0.0) += 0.2;
        }

        // ── Reflection 信号 ────────────────────────
        let reflection_signals = lower.starts_with("recent:")
            || lower == "route_stats" || lower == "route stats" || lower == "路由统计"
            || lower.starts_with("conflicts:")
            || lower == "improve" || lower == "优化建议";

        if reflection_signals {
            *raw.get_mut("reflection").unwrap_or(&mut 0.0) += 1.0;
            return;
        }

        // ── 数学运算 ──────────────────────────────
        let math_keywords = ["计算", "等于", "=", "+", "-", "*", "/"];
        if math_keywords.iter().any(|k| lower.contains(k)) {
            *raw.get_mut("conversation").unwrap_or(&mut 0.0) += 0.8;
            return;
        }

        // ── 默认 ──────────────────────────────────
        // 没有明显信号时，给 conversation 高权重
        if *raw.get("timeline").unwrap_or(&0.0) < 0.1
            && *raw.get("semantic").unwrap_or(&0.0) < 0.1
        {
            *raw.get_mut("conversation").unwrap_or(&mut 0.0) += 1.0;
        }
    }
}

// ─── 兼容层：旧的 MemoryRouter 包装为 ProbabilisticRouter ───

/// LegacyRouter — 将旧版 MemoryRouter 包装为 ProbabilisticRouter 兼容使用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryRoute {
    None,
    Conversation,
    Semantic,
    Episode,
    Deep,
}

/// 旧版 MemoryRouter（保持向后兼容）。
pub struct MemoryRouter {
    inner: ProbabilisticRouter,
}

impl Default for MemoryRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryRouter {
    pub fn new() -> Self {
        Self {
            inner: ProbabilisticRouter::new(),
        }
    }

    /// 判断是否需要记忆（旧版兼容）。
    pub fn route(&self, question: &str) -> MemoryRoute {
        let weights = self.inner.route(question);

        let conversation = *weights.get("conversation").unwrap_or(&0.0);
        let timeline = *weights.get("timeline").unwrap_or(&0.0);
        let semantic = *weights.get("semantic").unwrap_or(&0.0);
        let reflection = *weights.get("reflection").unwrap_or(&0.0);

        // softmax 归一化后权重之和为 1.0，使用相对比较而非绝对阈值
        let all_weights = [conversation, timeline, semantic, reflection];
        let max_weight = all_weights.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        // 最高权重的类别就是路由选择
        if reflection >= max_weight && reflection > 0.1 {
            return MemoryRoute::Episode;
        }
        if timeline >= max_weight && timeline > 0.1 {
            return MemoryRoute::Episode;
        }
        if semantic >= max_weight && semantic > 0.1 {
            return MemoryRoute::Semantic;
        }
        if conversation >= max_weight && conversation > 0.1 {
            return MemoryRoute::Conversation;
        }
        MemoryRoute::None
    }

    pub fn needs_memory(&self, question: &str) -> bool {
        self.route(question) != MemoryRoute::None
            && self.route(question) != MemoryRoute::Conversation
    }

    pub fn needs_deep_memory(&self, question: &str) -> bool {
        matches!(
            self.route(question),
            MemoryRoute::Episode | MemoryRoute::Deep
        )
    }

    pub fn needs_timeline(&self, question: &str) -> bool {
        let weights = self.inner.route(question);
        *weights.get("timeline").unwrap_or(&0.0) > 0.3
    }

    /// 访问内部的 ProbabilisticRouter。
    pub fn probabilistic(&self) -> &ProbabilisticRouter {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ProbabilisticRouter 测试 ───────────────────────

    #[test]
    fn test_probabilistic_route_greeting() {
        let router = ProbabilisticRouter::new();
        let weights = router.route("你好");
        // 问候应该走 conversation
        assert!(
            *weights.get("conversation").unwrap_or(&0.0) > 0.3,
            "greeting should route to conversation, got: {:?}", weights
        );
    }

    #[test]
    fn test_probabilistic_route_episode() {
        let router = ProbabilisticRouter::new();
        let weights = router.route("项目A为什么暂停");
        // "为什么" + "暂停" + "项目" = 3个timeline信号
        assert!(
            *weights.get("timeline").unwrap_or(&0.0) > 0.3,
            "why/pause question should route to timeline, got: {:?}", weights
        );
    }

    #[test]
    fn test_probabilistic_route_semantic() {
        let router = ProbabilisticRouter::new();
        let weights = router.route("什么是RAG");
        assert!(
            *weights.get("semantic").unwrap_or(&0.0) > 0.3,
            "definition question should route to semantic, got: {:?}", weights
        );
    }

    #[test]
    fn test_probabilistic_route_reflection() {
        let router = ProbabilisticRouter::new();
        let w1 = router.route("recent:10");
        assert!(*w1.get("reflection").unwrap_or(&0.0) > 0.3, "recent:10 should route to reflection");

        let w2 = router.route("route_stats");
        assert!(*w2.get("reflection").unwrap_or(&0.0) > 0.3, "route_stats should route to reflection");

        let w3 = router.route("优化建议");
        assert!(*w3.get("reflection").unwrap_or(&0.0) > 0.3, "优化建议 should route to reflection");
    }

    #[test]
    fn test_probabilistic_weights_sum_to_one() {
        let router = ProbabilisticRouter::new();
        let weights = router.route("项目A为什么暂停");
        let sum: f64 = weights.values().sum();
        assert!(
            (sum - 1.0).abs() < 0.01,
            "weights should sum to ~1.0, got: {}", sum
        );
    }

    #[test]
    fn test_sorted_weights() {
        let router = ProbabilisticRouter::new();
        let sorted = router.sorted_weights("项目A进展如何");
        assert!(!sorted.is_empty());
        // 检查降序排列
        for i in 1..sorted.len() {
            assert!(sorted[i - 1].1 >= sorted[i].1, "should be sorted descending");
        }
    }

    #[test]
    fn test_top_k() {
        let router = ProbabilisticRouter::new().with_top_k(2);
        let top = router.top_k_workflows("项目A为什么暂停");
        assert!(top.len() <= 2, "top-k should limit to 2, got {}", top.len());
    }

    #[test]
    fn test_temperature_effect() {
        let hot = ProbabilisticRouter::new().with_temperature(5.0);
        let cold = ProbabilisticRouter::new().with_temperature(0.5);

        let hot_w = hot.route("什么是RAG");
        let cold_w = cold.route("什么是RAG");

        // 低温应该更锐利（最大值更高）
        let hot_max = hot_w.values().cloned().fold(f64::NEG_INFINITY, f64::max);
        let cold_max = cold_w.values().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(cold_max >= hot_max, "cold temp should give sharper distribution");
    }

    #[test]
    fn test_threshold_filtering() {
        let router = ProbabilisticRouter::new().with_threshold(0.5);
        let weights = router.route("你好");
        // 只有高于 0.5 的保留
        for (_, w) in &weights {
            assert!(*w >= 0.5, "all weights should be >= threshold");
        }
    }

    #[test]
    fn test_custom_rule() {
        let mut router = ProbabilisticRouter::new();
        router.add_rule(Box::new(|q| {
            if q.contains("紧急") {
                let mut w = HashMap::new();
                w.insert("timeline".to_string(), 1.0);
                Some(w)
            } else {
                None
            }
        }));

        let weights = router.route("紧急查询项目A状态");
        assert!(
            *weights.get("timeline").unwrap_or(&0.0) > 0.9,
            "custom rule should override"
        );
    }

    #[test]
    fn test_needs_memory() {
        let router = ProbabilisticRouter::new();
        // 问候不需要memory
        assert!(!router.needs_memory("你好"), "greeting no memory needed");
        // 项目问题需要memory
        assert!(router.needs_memory("项目A进展如何"), "project question needs memory");
    }

    #[test]
    fn test_math_routing() {
        let router = ProbabilisticRouter::new();
        let weights = router.route("计算 125 + 37");
        assert!(
            *weights.get("conversation").unwrap_or(&0.0) > 0.3,
            "math should route to conversation"
        );
    }

    // ── 旧版 MemoryRouter 兼容测试 ─────────────────

    #[test]
    fn test_legacy_route_greeting() {
        let router = MemoryRouter::new();
        // softmax归一化后，问候路由到Conversation（和None一样返回空图）
        assert_eq!(router.route("你好"), MemoryRoute::Conversation);
        assert_eq!(router.route("hello"), MemoryRoute::Conversation);
    }

    #[test]
    fn test_legacy_route_episode() {
        let router = MemoryRouter::new();
        assert_eq!(router.route("项目A为什么暂停"), MemoryRoute::Episode);
    }

    #[test]
    fn test_legacy_route_semantic() {
        let router = MemoryRouter::new();
        assert_eq!(router.route("什么是RAG"), MemoryRoute::Semantic);
    }

    #[test]
    fn test_legacy_needs_memory() {
        let router = MemoryRouter::new();
        assert!(!router.needs_memory("你好"));
        assert!(router.needs_memory("项目A进展如何"));
    }

    #[test]
    fn test_legacy_needs_deep_memory() {
        let router = MemoryRouter::new();
        assert!(router.needs_deep_memory("项目A为什么暂停"));
        assert!(!router.needs_deep_memory("你好"));
    }
}
