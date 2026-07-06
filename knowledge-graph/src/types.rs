//! 知识图谱类型系统 — 参照 UA 设计，21 种节点 + 35 种边.
//!
//! 覆盖：代码结构、非代码配置、领域模型、知识类型。

use serde::{Deserialize, Serialize};

// ── Node Types (21) ────────────────────────────────

/// 节点类型（5 代码 + 8 非代码 + 3 领域 + 5 知识）.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeType {
    // ── 代码结构 (5) ──
    /// 文件.
    File,
    /// 函数/方法.
    Function,
    /// 类.
    Class,
    /// 模块/包.
    Module,
    /// 抽象概念.
    Concept,
    // ── 非代码 (8) ──
    /// 配置文件.
    Config,
    /// 文档.
    Document,
    /// 服务.
    Service,
    /// 数据库表.
    Table,
    /// API 端点.
    Endpoint,
    /// 流水线/CI.
    Pipeline,
    /// 数据模型/协议.
    Schema,
    /// 基础设施资源.
    Resource,
    // ── 领域 (3) ──
    /// 领域实体.
    Domain,
    /// 业务流程.
    Flow,
    /// 流程步骤.
    Step,
    // ── 知识 (5) ──
    /// 文章/文档.
    Article,
    /// 实体/概念.
    Entity,
    /// 主题.
    Topic,
    /// 论点/声明.
    Claim,
    /// 信息来源.
    Source,
}

impl NodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeType::File => "file",
            NodeType::Function => "function",
            NodeType::Class => "class",
            NodeType::Module => "module",
            NodeType::Concept => "concept",
            NodeType::Config => "config",
            NodeType::Document => "document",
            NodeType::Service => "service",
            NodeType::Table => "table",
            NodeType::Endpoint => "endpoint",
            NodeType::Pipeline => "pipeline",
            NodeType::Schema => "schema",
            NodeType::Resource => "resource",
            NodeType::Domain => "domain",
            NodeType::Flow => "flow",
            NodeType::Step => "step",
            NodeType::Article => "article",
            NodeType::Entity => "entity",
            NodeType::Topic => "topic",
            NodeType::Claim => "claim",
            NodeType::Source => "source",
        }
    }
}

// ── Edge Types (35) ────────────────────────────────

/// 边类型（8 大类 35 种）.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EdgeType {
    // ── 结构 (5) ──
    /// 导入依赖.
    Imports,
    /// 导出.
    Exports,
    /// 包含.
    Contains,
    /// 继承.
    Inherits,
    /// 实现.
    Implements,
    // ── 行为 (4) ──
    /// 调用.
    Calls,
    /// 订阅.
    Subscribes,
    /// 发布.
    Publishes,
    /// 中间件.
    Middleware,
    // ── 数据流 (4) ──
    /// 读.
    ReadsFrom,
    /// 写.
    WritesTo,
    /// 转换.
    Transforms,
    /// 校验.
    Validates,
    // ── 依赖 (3) ──
    /// 依赖于.
    DependsOn,
    /// 被测试.
    TestedBy,
    /// 配置.
    Configures,
    // ── 语义 (2) ──
    /// 关联.
    Related,
    /// 相似.
    SimilarTo,
    // ── 基础设施 (4) ──
    /// 部署.
    Deploys,
    /// 服务.
    Serves,
    /// 提供.
    Provisions,
    /// 触发.
    Triggers,
    // ── 领域 (3) ──
    /// 数据迁移.
    Migrates,
    /// 文档关联.
    Documents,
    /// 路由.
    Routes,
    /// 定义结构.
    DefinesSchema,
    /// 包含流程.
    ContainsFlow,
    /// 流程步骤.
    FlowStep,
    /// 跨领域.
    CrossDomain,
    // ── 知识 (5) ──
    /// 引用.
    Cites,
    /// 矛盾.
    Contradicts,
    /// 基于.
    BuildsOn,
    /// 示例.
    Exemplifies,
    /// 分类.
    CategorizedUnder,
    /// 作者.
    AuthoredBy,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::Imports => "imports",
            EdgeType::Exports => "exports",
            EdgeType::Contains => "contains",
            EdgeType::Inherits => "inherits",
            EdgeType::Implements => "implements",
            EdgeType::Calls => "calls",
            EdgeType::Subscribes => "subscribes",
            EdgeType::Publishes => "publishes",
            EdgeType::Middleware => "middleware",
            EdgeType::ReadsFrom => "reads_from",
            EdgeType::WritesTo => "writes_to",
            EdgeType::Transforms => "transforms",
            EdgeType::Validates => "validates",
            EdgeType::DependsOn => "depends_on",
            EdgeType::TestedBy => "tested_by",
            EdgeType::Configures => "configures",
            EdgeType::Related => "related",
            EdgeType::SimilarTo => "similar_to",
            EdgeType::Deploys => "deploys",
            EdgeType::Serves => "serves",
            EdgeType::Provisions => "provisions",
            EdgeType::Triggers => "triggers",
            EdgeType::Migrates => "migrates",
            EdgeType::Documents => "documents",
            EdgeType::Routes => "routes",
            EdgeType::DefinesSchema => "defines_schema",
            EdgeType::ContainsFlow => "contains_flow",
            EdgeType::FlowStep => "flow_step",
            EdgeType::CrossDomain => "cross_domain",
            EdgeType::Cites => "cites",
            EdgeType::Contradicts => "contradicts",
            EdgeType::BuildsOn => "builds_on",
            EdgeType::Exemplifies => "exemplifies",
            EdgeType::CategorizedUnder => "categorized_under",
            EdgeType::AuthoredBy => "authored_by",
        }
    }
}

// ── Core Data Structures ───────────────────────────

/// 边方向.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeDirection {
    Forward,
    Backward,
    Bidirectional,
}

/// 复杂度等级.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Complexity {
    Simple,
    Moderate,
    Complex,
}

/// 领域元数据.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DomainMeta {
    pub entities: Vec<String>,
    pub business_rules: Vec<String>,
    pub cross_domain_interactions: Vec<String>,
    pub entry_point: Option<String>,
}

/// 知识元数据.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeMeta {
    pub wikilinks: Vec<String>,
    pub backlinks: Vec<String>,
    pub category: Option<String>,
    pub content: Option<String>,
}

/// 图谱节点.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub node_type: NodeType,
    pub name: String,
    pub file_path: Option<String>,
    pub line_range: Option<[u32; 2]>,
    pub summary: String,
    pub tags: Vec<String>,
    pub complexity: Complexity,
    pub language: Option<String>,
    pub domain_meta: Option<DomainMeta>,
    pub knowledge_meta: Option<KnowledgeMeta>,
}

/// 图谱边.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
    pub direction: EdgeDirection,
    pub description: Option<String>,
    /// 权重 0.0–1.0.
    pub weight: f64,
}

/// 逻辑层.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub description: String,
    pub node_ids: Vec<String>,
}

/// 导览步骤.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourStep {
    pub order: u32,
    pub title: String,
    pub description: String,
    pub node_ids: Vec<String>,
    pub language_lesson: Option<String>,
}

/// 项目元数据.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub name: String,
    pub languages: Vec<String>,
    pub frameworks: Vec<String>,
    pub description: String,
    pub analyzed_at: String,
    pub git_commit_hash: String,
}

/// 知识图谱根结构.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub version: String,
    pub kind: GraphKind,
    pub project: ProjectMeta,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub layers: Vec<Layer>,
    pub tour: Vec<TourStep>,
}

impl KnowledgeGraph {
    pub fn new(project_name: &str, git_hash: &str) -> Self {
        Self {
            version: "1.0.0".into(),
            kind: GraphKind::Codebase,
            project: ProjectMeta {
                name: project_name.into(),
                languages: Vec::new(),
                frameworks: Vec::new(),
                description: String::new(),
                analyzed_at: chrono::Utc::now().to_rfc3339(),
                git_commit_hash: git_hash.into(),
            },
            nodes: Vec::new(),
            edges: Vec::new(),
            layers: Vec::new(),
            tour: Vec::new(),
        }
    }
}

/// 图谱类型.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GraphKind {
    Codebase,
    Knowledge,
    Agent,
    Custom(String),
}
