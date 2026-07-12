//! AgentSwarm — 核心类型定义
//!
//! 定义 SwarmAgent、SwarmState、SwarmConfig 等核心数据结构。
//! 支持群体智能中的动态角色分配、任务竞标和能力声明。

use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

// ── Swarm 配置 ──────────────────────────────────────

/// Swarm 协作策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwarmStrategy {
    /// 投票制 — 所有 Agent 投票决定最佳结果
    Voting,
    /// 共识制 — Agent 间达成共识（需要 majority）
    Consensus,
    /// 层级制 — Leader 分配任务，Worker 执行
    Hierarchical,
    /// 民主制 — 所有 Agent 平等参与决策
    Democratic,
    /// 竞标制 — Agent 竞标任务，最优者执行
    Bidding,
    /// 混合制 — 根据任务类型动态切换策略
    Hybrid,
}

impl SwarmStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            SwarmStrategy::Voting => "voting",
            SwarmStrategy::Consensus => "consensus",
            SwarmStrategy::Hierarchical => "hierarchical",
            SwarmStrategy::Democratic => "democratic",
            SwarmStrategy::Bidding => "bidding",
            SwarmStrategy::Hybrid => "hybrid",
        }
    }

    /// 是否需要多数决
    pub fn requires_majority(&self) -> bool {
        matches!(self, SwarmStrategy::Voting | SwarmStrategy::Consensus)
    }

    /// 是否需要 Leader
    pub fn requires_leader(&self) -> bool {
        matches!(self, SwarmStrategy::Hierarchical)
    }
}

impl std::fmt::Display for SwarmStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Swarm 拓扑结构
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwarmTopology {
    /// 星型 — 中心 Coordinator 连接所有 Agent
    Star,
    /// 网状 — 所有 Agent 两两互联
    Mesh,
    /// 环型 — Agent 形成环形通信
    Ring,
    /// 树型 — 层级树状结构
    Tree,
    /// 动态 — 根据任务自适应调整
    Dynamic,
}

impl SwarmTopology {
    pub fn as_str(&self) -> &'static str {
        match self {
            SwarmTopology::Star => "star",
            SwarmTopology::Mesh => "mesh",
            SwarmTopology::Ring => "ring",
            SwarmTopology::Tree => "tree",
            SwarmTopology::Dynamic => "dynamic",
        }
    }
}

// ── Swarm 配置 ──────────────────────────────────────

/// Swarm 引擎配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmConfig {
    /// Swarm 名称
    pub name: String,
    /// 协作策略
    pub strategy: SwarmStrategy,
    /// 拓扑结构
    pub topology: SwarmTopology,
    /// 最小 Agent 数量
    pub min_agents: usize,
    /// 最大 Agent 数量
    pub max_agents: usize,
    /// 任务超时
    pub task_timeout: Duration,
    /// 心跳间隔
    pub heartbeat_interval: Duration,
    /// 是否启用自治行为（自我反思、自我进化）
    pub enable_autonomy: bool,
    /// 是否启用 Emergent Specialization
    pub enable_emergent_specialization: bool,
    /// Agent 空闲回收时间
    pub idle_timeout: Duration,
    /// 共识阈值（0.0~1.0），例如 0.67 表示 2/3 多数
    pub consensus_threshold: f64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 是否记录详细审计
    pub enable_audit_trail: bool,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            name: "default-swarm".to_string(),
            strategy: SwarmStrategy::Democratic,
            topology: SwarmTopology::Mesh,
            min_agents: 2,
            max_agents: 16,
            task_timeout: Duration::from_secs(300),
            heartbeat_interval: Duration::from_secs(5),
            enable_autonomy: true,
            enable_emergent_specialization: true,
            idle_timeout: Duration::from_secs(600),
            consensus_threshold: 0.67,
            max_retries: 3,
            enable_audit_trail: true,
        }
    }
}

// ── Agent 角色定义 ──────────────────────────────────

/// 增强的 Agent 角色（超越基础 Planner/Executor）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwarmAgentRole {
    /// 🧠 分析者 — 分析任务、拆分问题
    Analyst,
    /// 🎨 创造者 — 生成创意、编写代码、创作内容
    Creator,
    /// 🔍 验证者 — 验证结果、检查质量
    Validator,
    /// 🤝 协商者 — 协调冲突、达成共识
    Negotiator,
    /// 👁 观察者 — 监控 Swarm 健康、记录行为
    Observer,
    /// 📋 规划者 — 制定执行计划
    Planner,
    /// ⚡ 执行者 — 执行具体任务
    Executor,
    /// 🧪 测试者 — 测试输出、检测缺陷
    Tester,
    /// 📊 聚合者 — 聚合和总结多 Agent 输出
    Aggregator,
    /// 🎯 路由者 — 将任务路由到最适合的 Agent
    Router,
}

impl SwarmAgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            SwarmAgentRole::Analyst => "analyst",
            SwarmAgentRole::Creator => "creator",
            SwarmAgentRole::Validator => "validator",
            SwarmAgentRole::Negotiator => "negotiator",
            SwarmAgentRole::Observer => "observer",
            SwarmAgentRole::Planner => "planner",
            SwarmAgentRole::Executor => "executor",
            SwarmAgentRole::Tester => "tester",
            SwarmAgentRole::Aggregator => "aggregator",
            SwarmAgentRole::Router => "router",
        }
    }

    /// 获取角色描述
    pub fn description(&self) -> &'static str {
        match self {
            SwarmAgentRole::Analyst => "Analyzes tasks and breaks down problems",
            SwarmAgentRole::Creator => "Generates creative content and code",
            SwarmAgentRole::Validator => "Validates results and checks quality",
            SwarmAgentRole::Negotiator => "Coordinates conflicts and builds consensus",
            SwarmAgentRole::Observer => "Monitors swarm health and records behavior",
            SwarmAgentRole::Planner => "Creates execution plans",
            SwarmAgentRole::Executor => "Executes specific tasks",
            SwarmAgentRole::Tester => "Tests outputs and detects defects",
            SwarmAgentRole::Aggregator => "Aggregates and summarizes multi-agent outputs",
            SwarmAgentRole::Router => "Routes tasks to best-suited agents",
        }
    }
}

impl std::fmt::Display for SwarmAgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── Swarm Agent ─────────────────────────────────────

/// Swarm 中的 Agent 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwarmAgentStatus {
    /// 空闲 — 等待任务
    Idle,
    /// 忙碌 — 正在执行任务
    Busy,
    /// 评估中 — 正在评估任务是否接受
    Evaluating,
    /// 故障 — 发生错误
    Faulted,
    /// 离线 — 暂时不可用
    Offline,
    /// 已移除 — 从 Swarm 中移除
    Removed,
}

/// Swarm Agent 实例
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmAgent {
    /// Agent ID
    pub id: LsId,
    /// Agent 名称
    pub name: String,
    /// 当前角色
    pub role: SwarmAgentRole,
    /// 备选角色列表（Emergent Specialization）
    pub alternative_roles: Vec<SwarmAgentRole>,
    /// 状态
    pub status: SwarmAgentStatus,
    /// 能力评分（0.0~1.0）
    pub capability_score: f64,
    /// 历史成功率
    pub success_rate: f64,
    /// 任务完成数
    pub tasks_completed: u64,
    /// 平均执行时长
    pub avg_execution_ms: f64,
    /// 领域专长（key=领域, value=熟练度 0.0~1.0）
    pub expertise: HashMap<String, f64>,
    /// 最后活跃时间
    pub last_active: i64,
    /// 加入 Swarm 时间
    pub joined_at: i64,
    /// Agent 元数据
    pub metadata: HashMap<String, String>,
}

impl SwarmAgent {
    pub fn new(name: impl Into<String>, role: SwarmAgentRole) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: LsId::new(),
            name: name.into(),
            role,
            alternative_roles: Vec::new(),
            status: SwarmAgentStatus::Idle,
            capability_score: 0.5,
            success_rate: 1.0,
            tasks_completed: 0,
            avg_execution_ms: 0.0,
            expertise: HashMap::new(),
            last_active: now,
            joined_at: now,
            metadata: HashMap::new(),
        }
    }

    pub fn with_expertise(mut self, domain: &str, score: f64) -> Self {
        self.expertise.insert(domain.to_string(), score);
        self
    }

    pub fn with_alternative_role(mut self, role: SwarmAgentRole) -> Self {
        self.alternative_roles.push(role);
        self
    }

    /// Agent 是否可用于任务
    pub fn is_available(&self) -> bool {
        matches!(self.status, SwarmAgentStatus::Idle)
    }

    /// 更新能力评分（指数移动平均）
    pub fn update_capability(&mut self, success: bool, execution_ms: f64) {
        let alpha = 0.3;
        let task_score = if success { 1.0 } else { 0.0 };
        self.capability_score = alpha * task_score + (1.0 - alpha) * self.capability_score;
        self.success_rate = alpha * (if success { 1.0 } else { 0.0 }) + (1.0 - alpha) * self.success_rate;
        self.tasks_completed += 1;
        self.avg_execution_ms = if self.tasks_completed == 1 {
            execution_ms
        } else {
            0.9 * self.avg_execution_ms + 0.1 * execution_ms
        };
        self.last_active = chrono::Utc::now().timestamp();
    }
}

// ── Swarm 任务 ──────────────────────────────────────

/// Swarm 任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    /// 任务 ID
    pub id: LsId,
    /// 任务名称
    pub name: String,
    /// 任务描述
    pub description: String,
    /// 输入数据
    pub input: serde_json::Value,
    /// 所需角色（None 表示任意角色）
    pub required_role: Option<SwarmAgentRole>,
    /// 所需专长
    pub required_expertise: Vec<String>,
    /// 优先级（0=最低, 10=最高）
    pub priority: u8,
    /// 最大竞标 Agent 数
    pub max_bidders: usize,
    /// 依赖的任务 ID 列表
    pub depends_on: Vec<LsId>,
    /// 创建时间
    pub created_at: i64,
    /// 超时
    pub timeout_secs: u64,
}

impl SwarmTask {
    pub fn new(name: impl Into<String>, description: impl Into<String>, input: serde_json::Value) -> Self {
        Self {
            id: LsId::new(),
            name: name.into(),
            description: description.into(),
            input,
            required_role: None,
            required_expertise: Vec::new(),
            priority: 5,
            max_bidders: 3,
            depends_on: Vec::new(),
            created_at: chrono::Utc::now().timestamp(),
            timeout_secs: 300,
        }
    }

    pub fn with_required_role(mut self, role: SwarmAgentRole) -> Self {
        self.required_role = Some(role);
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    pub fn with_dependency(mut self, dep_id: LsId) -> Self {
        self.depends_on.push(dep_id);
        self
    }
}

/// 任务执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTaskResult {
    /// 任务 ID
    pub task_id: LsId,
    /// 执行 Agent ID
    pub agent_id: LsId,
    /// Agent 名称
    pub agent_name: String,
    /// 输出数据
    pub output: serde_json::Value,
    /// 是否成功
    pub success: bool,
    /// 执行耗时 ms
    pub execution_ms: u64,
    /// 信心分数（0.0~1.0）
    pub confidence: f64,
    /// 错误信息
    pub error: Option<String>,
    /// 开始时间
    pub started_at: i64,
    /// 完成时间
    pub completed_at: i64,
}

// ── Swarm 状态 ──────────────────────────────────────

/// Swarm 整体状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmState {
    /// Swarm ID
    pub id: LsId,
    /// Swarm 名称
    pub name: String,
    /// 当前策略
    pub strategy: SwarmStrategy,
    /// 当前拓扑
    pub topology: SwarmTopology,
    /// Agent 列表
    pub agents: Vec<SwarmAgent>,
    /// 活跃任务数
    pub active_tasks: u32,
    /// 总任务完成数
    pub total_tasks_completed: u64,
    /// 总任务失败数
    pub total_tasks_failed: u64,
    /// 总体成功率
    pub overall_success_rate: f64,
    /// 启动时间
    pub started_at: i64,
    /// 最后状态更新时间
    pub last_updated: i64,
    /// Swarm 元数据
    pub metadata: HashMap<String, String>,
}

impl SwarmState {
    pub fn new(name: impl Into<String>, strategy: SwarmStrategy, topology: SwarmTopology) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: LsId::new(),
            name: name.into(),
            strategy,
            topology,
            agents: Vec::new(),
            active_tasks: 0,
            total_tasks_completed: 0,
            total_tasks_failed: 0,
            overall_success_rate: 1.0,
            started_at: now,
            last_updated: now,
            metadata: HashMap::new(),
        }
    }

    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    pub fn available_agents(&self) -> Vec<&SwarmAgent> {
        self.agents.iter().filter(|a| a.is_available()).collect()
    }

    pub fn available_agent_count(&self) -> usize {
        self.agents.iter().filter(|a| a.is_available()).count()
    }

    pub fn busy_agent_count(&self) -> usize {
        self.agents.iter().filter(|a| a.status == SwarmAgentStatus::Busy).count()
    }
}

// ── Swarm 竞标 ──────────────────────────────────────

/// Agent 对任务的竞标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmBid {
    /// Agent ID
    pub agent_id: LsId,
    /// Agent 名称
    pub agent_name: String,
    /// 报价（信心分数 0.0~1.0）
    pub bid_score: f64,
    /// 预计执行时间 ms
    pub estimated_ms: u64,
    /// 竞标理由
    pub rationale: String,
    /// 竞标时间
    pub timestamp: i64,
}

/// 投票记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmVote {
    /// Agent ID
    pub agent_id: LsId,
    /// 投票值（支持/反对/弃权）
    pub vote: VoteValue,
    /// 投票理由
    pub rationale: String,
    /// 投票时间
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteValue {
    Yes,
    No,
    Abstain,
}

// ── Consensus 结果 ─────────────────────────────────

/// 共识决策结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    /// 是否达成共识
    pub achieved: bool,
    /// 总投票数
    pub total_votes: usize,
    /// 赞成票
    pub yes_votes: usize,
    /// 反对票
    pub no_votes: usize,
    /// 弃权票
    pub abstain_votes: usize,
    /// 支持率
    pub approval_ratio: f64,
    /// 决策内容
    pub decision: serde_json::Value,
    /// 少数意见
    pub minority_opinions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SwarmStrategy ────────────────────────────────

    #[test]
    fn test_swarm_strategy_roundtrip() {
        for s in &[SwarmStrategy::Voting, SwarmStrategy::Consensus,
                   SwarmStrategy::Hierarchical, SwarmStrategy::Democratic,
                   SwarmStrategy::Bidding, SwarmStrategy::Hybrid] {
            let json = serde_json::to_string(s).unwrap();
            let deserialized: SwarmStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, deserialized);
        }
    }

    #[test]
    fn test_swarm_strategy_as_str() {
        assert_eq!(SwarmStrategy::Voting.as_str(), "voting");
        assert_eq!(SwarmStrategy::Consensus.as_str(), "consensus");
        assert_eq!(SwarmStrategy::Hybrid.as_str(), "hybrid");
    }

    #[test]
    fn test_swarm_strategy_requires_majority() {
        assert!(SwarmStrategy::Voting.requires_majority());
        assert!(SwarmStrategy::Consensus.requires_majority());
        assert!(!SwarmStrategy::Hierarchical.requires_majority());
        assert!(!SwarmStrategy::Bidding.requires_majority());
    }

    #[test]
    fn test_swarm_strategy_requires_leader() {
        assert!(SwarmStrategy::Hierarchical.requires_leader());
        assert!(!SwarmStrategy::Voting.requires_leader());
        assert!(!SwarmStrategy::Democratic.requires_leader());
    }

    #[test]
    fn test_swarm_strategy_display() {
        assert_eq!(format!("{}", SwarmStrategy::Bidding), "bidding");
    }

    // ── SwarmTopology ────────────────────────────────

    #[test]
    fn test_swarm_topology_roundtrip() {
        for t in &[SwarmTopology::Star, SwarmTopology::Mesh,
                   SwarmTopology::Ring, SwarmTopology::Tree,
                   SwarmTopology::Dynamic] {
            let json = serde_json::to_string(t).unwrap();
            let deserialized: SwarmTopology = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, deserialized);
        }
    }

    #[test]
    fn test_swarm_topology_as_str() {
        assert_eq!(SwarmTopology::Star.as_str(), "star");
        assert_eq!(SwarmTopology::Mesh.as_str(), "mesh");
        assert_eq!(SwarmTopology::Dynamic.as_str(), "dynamic");
    }

    // ── SwarmConfig ──────────────────────────────────

    #[test]
    fn test_swarm_config_default() {
        let cfg = SwarmConfig::default();
        assert_eq!(cfg.name, "default-swarm");
        assert_eq!(cfg.strategy, SwarmStrategy::Democratic);
        assert_eq!(cfg.topology, SwarmTopology::Mesh);
        assert_eq!(cfg.min_agents, 2);
        assert_eq!(cfg.max_agents, 16);
        assert!(cfg.enable_autonomy);
        assert_eq!(cfg.consensus_threshold, 0.67);
        assert_eq!(cfg.max_retries, 3);
    }

    #[test]
    fn test_swarm_config_serialization() {
        let cfg = SwarmConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: SwarmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, cfg.name);
        assert_eq!(deserialized.task_timeout, cfg.task_timeout);
    }

    // ── SwarmAgentRole ───────────────────────────────

    #[test]
    fn test_swarm_agent_role_roundtrip() {
        for r in &[SwarmAgentRole::Analyst, SwarmAgentRole::Creator,
                   SwarmAgentRole::Validator, SwarmAgentRole::Negotiator,
                   SwarmAgentRole::Observer, SwarmAgentRole::Planner,
                   SwarmAgentRole::Executor, SwarmAgentRole::Tester,
                   SwarmAgentRole::Aggregator, SwarmAgentRole::Router] {
            let json = serde_json::to_string(r).unwrap();
            let deserialized: SwarmAgentRole = serde_json::from_str(&json).unwrap();
            assert_eq!(*r, deserialized);
        }
    }

    #[test]
    fn test_swarm_agent_role_as_str() {
        assert_eq!(SwarmAgentRole::Analyst.as_str(), "analyst");
        assert_eq!(SwarmAgentRole::Creator.as_str(), "creator");
        assert_eq!(SwarmAgentRole::Router.as_str(), "router");
    }

    #[test]
    fn test_swarm_agent_role_description() {
        assert!(SwarmAgentRole::Analyst.description().contains("Analyzes"));
        assert!(SwarmAgentRole::Creator.description().contains("creative"));
    }

    #[test]
    fn test_swarm_agent_role_display() {
        assert_eq!(format!("{}", SwarmAgentRole::Tester), "tester");
    }

    // ── SwarmAgent ───────────────────────────────────

    #[test]
    fn test_swarm_agent_new() {
        let agent = SwarmAgent::new("agent-1", SwarmAgentRole::Planner);
        assert_eq!(agent.name, "agent-1");
        assert_eq!(agent.role, SwarmAgentRole::Planner);
        assert_eq!(agent.status, SwarmAgentStatus::Idle);
        assert_eq!(agent.capability_score, 0.5);
        assert_eq!(agent.success_rate, 1.0);
        assert_eq!(agent.tasks_completed, 0);
        assert!(!agent.id.is_nil());
    }

    #[test]
    fn test_swarm_agent_with_expertise() {
        let agent = SwarmAgent::new("agent-2", SwarmAgentRole::Executor)
            .with_expertise("rust", 0.9)
            .with_expertise("python", 0.7);
        assert_eq!(agent.expertise.len(), 2);
        assert_eq!(agent.expertise.get("rust").unwrap(), &0.9);
    }

    #[test]
    fn test_swarm_agent_with_alternative_role() {
        let agent = SwarmAgent::new("agent-3", SwarmAgentRole::Executor)
            .with_alternative_role(SwarmAgentRole::Validator)
            .with_alternative_role(SwarmAgentRole::Tester);
        assert_eq!(agent.alternative_roles.len(), 2);
    }

    #[test]
    fn test_swarm_agent_is_available() {
        let mut agent = SwarmAgent::new("test", SwarmAgentRole::Executor);
        assert!(agent.is_available());
        agent.status = SwarmAgentStatus::Busy;
        assert!(!agent.is_available());
        agent.status = SwarmAgentStatus::Faulted;
        assert!(!agent.is_available());
        agent.status = SwarmAgentStatus::Offline;
        assert!(!agent.is_available());
        agent.status = SwarmAgentStatus::Evaluating;
        assert!(!agent.is_available());
    }

    #[test]
    fn test_swarm_agent_update_capability_success() {
        let mut agent = SwarmAgent::new("test", SwarmAgentRole::Executor);
        agent.update_capability(true, 100.0);
        assert_eq!(agent.tasks_completed, 1);
        // capability_score = 0.3 * 1.0 + 0.7 * 0.5 = 0.65
        assert!((agent.capability_score - 0.65).abs() < 0.001);
        // success_rate = 0.3 * 1.0 + 0.7 * 1.0 = 1.0
        assert!((agent.success_rate - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_swarm_agent_update_capability_failure() {
        let mut agent = SwarmAgent::new("test", SwarmAgentRole::Executor);
        agent.update_capability(false, 200.0);
        assert_eq!(agent.tasks_completed, 1);
        assert!((agent.capability_score - 0.35).abs() < 0.001);
        assert!((agent.success_rate - 0.7).abs() < 0.001); // 0.3*0.0 + 0.7*1.0 = 0.7
    }

    #[test]
    fn test_swarm_agent_update_capability_multiple() {
        let mut agent = SwarmAgent::new("test", SwarmAgentRole::Executor);
        agent.update_capability(true, 100.0);
        agent.update_capability(true, 150.0);
        agent.update_capability(false, 200.0);
        assert_eq!(agent.tasks_completed, 3);
        // avg_execution_ms after 3: 0.9*100 + 0.1*150 = 105, then 0.9*105 + 0.1*200 = 114.5
        assert!((agent.avg_execution_ms - 114.5).abs() < 0.1);
    }

    #[test]
    fn test_swarm_agent_serialization() {
        let agent = SwarmAgent::new("test", SwarmAgentRole::Analyst)
            .with_expertise("go", 0.8);
        let json = serde_json::to_string(&agent).unwrap();
        let deserialized: SwarmAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.role, SwarmAgentRole::Analyst);
    }

    // ── SwarmAgentStatus ─────────────────────────────

    #[test]
    fn test_swarm_agent_status_roundtrip() {
        for s in &[SwarmAgentStatus::Idle, SwarmAgentStatus::Busy,
                   SwarmAgentStatus::Evaluating, SwarmAgentStatus::Faulted,
                   SwarmAgentStatus::Offline, SwarmAgentStatus::Removed] {
            let json = serde_json::to_string(s).unwrap();
            let deserialized: SwarmAgentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, deserialized);
        }
    }

    // ── SwarmTask ────────────────────────────────────

    #[test]
    fn test_swarm_task_new() {
        let task = SwarmTask::new("task-1", "Do something", serde_json::json!({"key": "val"}));
        assert_eq!(task.name, "task-1");
        assert_eq!(task.description, "Do something");
        assert_eq!(task.priority, 5);
        assert_eq!(task.max_bidders, 3);
        assert_eq!(task.timeout_secs, 300);
        assert!(task.required_role.is_none());
        assert!(task.depends_on.is_empty());
    }

    #[test]
    fn test_swarm_task_builder() {
        let dep_id = LsId::new();
        let task = SwarmTask::new("task-2", "High priority", serde_json::json!({}))
            .with_required_role(SwarmAgentRole::Creator)
            .with_priority(9)
            .with_dependency(dep_id);
        assert_eq!(task.required_role, Some(SwarmAgentRole::Creator));
        assert_eq!(task.priority, 9);
        assert_eq!(task.depends_on.len(), 1);
        assert_eq!(task.depends_on[0], dep_id);
    }

    #[test]
    fn test_swarm_task_priority_capped() {
        let task = SwarmTask::new("t", "", serde_json::json!({}))
            .with_priority(15);
        assert_eq!(task.priority, 10);
    }

    #[test]
    fn test_swarm_task_serialization() {
        let task = SwarmTask::new("task-3", "Test", serde_json::json!({"x": 1}));
        let json = serde_json::to_string(&task).unwrap();
        let deserialized: SwarmTask = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "task-3");
        assert!(!deserialized.id.is_nil());
    }

    // ── SwarmTaskResult ──────────────────────────────

    #[test]
    fn test_swarm_task_result_serialization() {
        let result = SwarmTaskResult {
            task_id: LsId::new(),
            agent_id: LsId::new(),
            agent_name: "agent-1".into(),
            output: serde_json::json!({"result": "done"}),
            success: true,
            execution_ms: 150,
            confidence: 0.95,
            error: None,
            started_at: 1000,
            completed_at: 1150,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: SwarmTaskResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.success);
        assert_eq!(deserialized.execution_ms, 150);
        assert_eq!(deserialized.output["result"], "done");
    }

    #[test]
    fn test_swarm_task_result_failed() {
        let result = SwarmTaskResult {
            task_id: LsId::new(),
            agent_id: LsId::new(),
            agent_name: "agent-1".into(),
            output: serde_json::json!(null),
            success: false,
            execution_ms: 50,
            confidence: 0.0,
            error: Some("timeout".into()),
            started_at: 1000,
            completed_at: 1050,
        };
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("timeout"));
    }

    // ── SwarmState ───────────────────────────────────

    #[test]
    fn test_swarm_state_new() {
        let state = SwarmState::new("my-swarm", SwarmStrategy::Voting, SwarmTopology::Star);
        assert_eq!(state.name, "my-swarm");
        assert_eq!(state.strategy, SwarmStrategy::Voting);
        assert_eq!(state.topology, SwarmTopology::Star);
        assert_eq!(state.agent_count(), 0);
        assert_eq!(state.overall_success_rate, 1.0);
        assert!(!state.id.is_nil());
    }

    #[test]
    fn test_swarm_state_agent_counts() {
        let mut state = SwarmState::new("test", SwarmStrategy::Democratic, SwarmTopology::Mesh);
        let agent = SwarmAgent::new("a1", SwarmAgentRole::Executor);
        state.agents.push(agent);
        assert_eq!(state.agent_count(), 1);
        assert_eq!(state.available_agent_count(), 1);
        assert_eq!(state.busy_agent_count(), 0);

        let mut busy = SwarmAgent::new("a2", SwarmAgentRole::Planner);
        busy.status = SwarmAgentStatus::Busy;
        state.agents.push(busy);
        assert_eq!(state.agent_count(), 2);
        assert_eq!(state.available_agent_count(), 1);
        assert_eq!(state.busy_agent_count(), 1);
    }

    #[test]
    fn test_swarm_state_available_agents() {
        let mut state = SwarmState::new("test", SwarmStrategy::Bidding, SwarmTopology::Ring);
        let a1 = SwarmAgent::new("a1", SwarmAgentRole::Executor);
        let mut a2 = SwarmAgent::new("a2", SwarmAgentRole::Planner);
        a2.status = SwarmAgentStatus::Busy;
        state.agents.push(a1);
        state.agents.push(a2);
        let available = state.available_agents();
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].name, "a1");
    }

    #[test]
    fn test_swarm_state_serialization() {
        let state = SwarmState::new("s1", SwarmStrategy::Consensus, SwarmTopology::Tree);
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: SwarmState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "s1");
    }

    // ── Bid / Vote / ConsensusResult ─────────────────

    #[test]
    fn test_swarm_bid_serialization() {
        let bid = SwarmBid {
            agent_id: LsId::new(),
            agent_name: "agent-1".into(),
            bid_score: 0.85,
            estimated_ms: 200,
            rationale: "Best fit for task".into(),
            timestamp: 1000,
        };
        let json = serde_json::to_string(&bid).unwrap();
        let deserialized: SwarmBid = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.bid_score, 0.85);
        assert_eq!(deserialized.agent_name, "agent-1");
    }

    #[test]
    fn test_vote_value_roundtrip() {
        for v in &[VoteValue::Yes, VoteValue::No, VoteValue::Abstain] {
            let json = serde_json::to_string(v).unwrap();
            let deserialized: VoteValue = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, deserialized);
        }
    }

    #[test]
    fn test_swarm_vote_serialization() {
        let vote = SwarmVote {
            agent_id: LsId::new(),
            vote: VoteValue::Yes,
            rationale: "Agree".into(),
            timestamp: 1000,
        };
        let json = serde_json::to_string(&vote).unwrap();
        let deserialized: SwarmVote = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.vote, VoteValue::Yes);
    }

    #[test]
    fn test_consensus_result_achieved() {
        let result = ConsensusResult {
            achieved: true,
            total_votes: 5,
            yes_votes: 4,
            no_votes: 1,
            abstain_votes: 0,
            approval_ratio: 0.8,
            decision: serde_json::json!({"action": "proceed"}),
            minority_opinions: vec!["Risk too high".into()],
        };
        assert!(result.achieved);
        assert_eq!(result.approval_ratio, 0.8);
        assert_eq!(result.minority_opinions.len(), 1);
    }

    #[test]
    fn test_consensus_result_not_achieved() {
        let result = ConsensusResult {
            achieved: false,
            total_votes: 3,
            yes_votes: 1,
            no_votes: 2,
            abstain_votes: 0,
            approval_ratio: 0.33,
            decision: serde_json::json!(null),
            minority_opinions: vec![],
        };
        assert!(!result.achieved);
    }

    #[test]
    fn test_consensus_result_serialization() {
        let result = ConsensusResult {
            achieved: true,
            total_votes: 2,
            yes_votes: 2,
            no_votes: 0,
            abstain_votes: 0,
            approval_ratio: 1.0,
            decision: serde_json::json!("ok"),
            minority_opinions: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ConsensusResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.achieved);
    }
}
