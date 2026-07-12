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
