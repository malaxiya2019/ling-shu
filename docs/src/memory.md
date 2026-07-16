# Memory System（记忆系统）

LingShu v6 的记忆系统基于 **Holographic Memory Architecture（全息记忆架构）** 设计，遵循"存储和回忆分离"原则。

## 架构总览

```
Agent Runtime
       |
QueryMemoryTool
       |
  MemoryRuntime
       |
┌──────┼──────────┐
│      │          │
Router Workflow  EvidenceGraph
│      │          │
│      │     Serializer/Consumer
│      │          │
│ Episode  Timeline  Semantic │
│ Store  Workflow  Workflow  │
│      │          │
│ Consolidation  │
│ Workflow       │
└──────┴──────────┘
       |
  Persistent Layer
  (SQLite / InMemory / TfIdfIndex)
```

## Crates（8个）

| Crate | 职责 | 状态 |
|---|---|---|
| `lingshu-memory-episode` | Episode 数据模型 + Repository trait | ✅ 完成 |
| `lingshu-memory-episode-sqlite` | SQLite 持久化实现（FTS5 + LIKE） | ✅ 完成 |
| `lingshu-evidence-graph` | Memory ABI — 统一输出格式 | ✅ 完成 |
| `lingshu-memory-workflow` | Router + Workflow 注册表 + 抽象 | ✅ 完成 |
| `lingshu-workflow-memory` | TimelineWorkflow 实现 | ✅ 完成 |
| `lingshu-memory-eval` | 评测框架（Recall/Precision/F1/Latency） | ✅ 完成 |
| `lingshu-memory-semantic` | TF-IDF 语义索引 + SemanticWorkflow | ✅ 完成 |
| `lingshu-memory-consolidation` | 记忆巩固（3 种策略 + ImportanceScorer） | ✅ 完成 |

## 多层记忆架构

### Layer 1: Conversation Memory（会话记忆）
- 最近 N 轮对话上下文
- 由 Agent Runtime 管理

### Layer 2: Semantic Memory（语义记忆）— ✅ 已实现
- TF-IDF 向量检索（纯本地，无需外部 API）
- 中文字符 n-gram 分词（单字 + bi-gram）
- 用于概念级查询（"什么是 RAG"、"张三"、"Rust"）

### Layer 3: Episode Memory（事件记忆）— ✅ 已实现
- 带时间戳的事件记录
- 实体关联（人物、项目、组织）
- 状态变更追踪
- SQLite 持久化（FTS5 + LIKE 组合搜索）

### Layer 4: Reasoning Graph（推理图）— 待实现
- 因果关系
- 决策推理
- 跨事件关联

## 核心设计决策

### 1. EvidenceGraph = Memory ABI
所有 MemoryWorkflow 统一返回 `EvidenceGraph`，LLM 只是消费者之一：

```rust
pub fn query(&self, question: &str) -> LsResult<EvidenceGraph>;
```

未来可以扩展消费者：Agent、UI、Analytics、Reasoning Engine。

### 2. 三层查询 Pipeline
```
User Question
       |
 MemoryRouter（判断是否需要记忆）
       |
 MemoryWorkflow（执行具体策略）
       |
 EvidenceGraph（统一输出）
```

### 3. MemoryRouter 规则路由
Router 基于关键词规则判断，无需 LLM 调用：
- `None` — 问候、数学、翻译
- `Semantic` — 概念查询走 SemanticWorkflow
- `Episode` — 上周、去年、项目、历史
- `Deep` — 复杂因果问题（预留）

### 4. 存储独立于推理
- EpisodeRepository trait 抽象
- 当前实现：InMemory + SQLite
- 可扩展：Postgres、Qdrant、TaoStorage

## Sprint 1: Memory Runtime MVP

### 新增 crate（4个）
- `lingshu-memory-episode` — 13 个测试
- `lingshu-evidence-graph` — 14 个测试
- `lingshu-memory-workflow` — 10 个测试
- `lingshu-workflow-memory` — 6 个测试

### EvidenceGraph = Memory ABI

```rust
pub struct EvidenceGraph {
    pub nodes: Vec<Node>,      // Event / Fact / Entity
    pub edges: Vec<Edge>,      // Temporal / Related / StateChange
    pub metadata: GraphMetadata,
}
```

## Sprint 2: SQLite 持久化

### 新增 crate
- `lingshu-memory-episode-sqlite` — 18 个测试

### SQLite + LIKE + FTS5 组合

```sql
-- FTS5 虚拟表（英文/结构化文本）
CREATE VIRTUAL TABLE episode_fts USING fts5(title, summary);

-- LIKE 回退（中文模糊搜索）
SELECT * FROM episodes WHERE title LIKE '%项目%' OR summary LIKE '%暂停%';
```

## Sprint 3-A: Memory Evaluation

### 新增 crate
- `lingshu-memory-eval` — 32 个测试

### 评测框架

```rust
let dataset = EvaluationDataset::projects();
let result = evaluator.evaluate(&dataset).await?;
// recall, precision, f1_score, latency_ms, token_cost
```

内置 6 个评测集：
- Projects（项目事件）
- Persons（人员关系）
- Timelines（时间线）
- StateChanges（状态变更）
- Mixed（混合查询）
- EdgeCases（边界情况）

## Sprint 3-B: Memory Consolidation（记忆巩固）

### 新增 crate
- `lingshu-memory-consolidation` — 63 个测试

### 核心架构

```
短期 Episode（零散事件流）
       │
       ▼
  ConsolidationEngine
       │
  ┌────┴────┐
  │         │
Analyzer  Strategies
  │     ┌───┼───┐
  │     │   │   │
  │  Summarize Dedup Profile
  │         │
  └────┬────┘
       ▼
ImportanceScorer（5 维评分）
  ├── 时效性（指数衰减，半衰期 7 天）
  ├── 实体丰富度
  ├── 源数量
  ├── 置信度
  └── 策略权重
       │
       ▼
ConsolidatedMemory
       │
       ▼
  EpisodeRepository（带 "consolidated" 标签持久化）
```

### 3 种巩固策略

| 策略 | 功能 | 场景 |
|---|---|---|
| **SummarizeStrategy** | 合并多个 Episode 为一条带时间线的总结 | 每日/每周事件汇总 |
| **DedupStrategy** | 检测标题+时间相近的重复 Episode | 重复日志去重 |
| **ProfileStrategy** | 按实体提取完整事件画像 | 人物/项目画像 |

### ImportanceScorer — 5 维评分

| 维度 | 权重 | 计算方式 |
|---|---|---|
| 时效性 | 0.30 | 指数衰减，半衰期 168h |
| 实体丰富度 | 0.15 | 实体数量 / 阈值 |
| 源数量 | 0.15 | 归一化到 0.33/次 |
| 置信度 | 0.15 | 直接使用 episode 置信度 |
| 策略权重 | 0.25 | Profile=0.10, Summarize=0.05, Dedup=0.02 |

### ConsolidationWorkflow

实现 `MemoryWorkflow` trait，流程：
1. 从 EpisodeRepository 获取未巩固的 Episode
2. ConsolidationEngine 执行策略
3. ImportanceScorer 评分
4. 持久化为 ConsolidatedMemory
5. 构建 EvidenceGraph（节点携带 importance/level/strategy 标签）

### 存储层

```rust
pub trait ConsolidatedMemoryRepository: Send + Sync {
    async fn store_consolidated(&self, memory: &ConsolidatedMemory) -> LsResult<()>;
    async fn get_consolidated(&self, id: &str) -> LsResult<Option<ConsolidatedMemory>>;
    async fn list_consolidated(&self, limit: usize, offset: usize) -> LsResult<Vec<ConsolidatedMemory>>;
    async fn list_by_strategy(&self, strategy: &str, ...) -> LsResult<Vec<ConsolidatedMemory>>;
    async fn list_by_importance(&self, min_score: f64, ...) -> LsResult<Vec<ConsolidatedMemory>>;
    async fn delete_consolidated(&self, id: &str) -> LsResult<bool>;
}
```

两种实现：
- `EpisodeBackedConsolidatedStore` — 复用 EpisodeRepository（SQLite/InMemory）
- `InMemoryConsolidatedStore` — 测试用

## Sprint 3-C: Semantic Memory（语义记忆）

### 新增 crate
- `lingshu-memory-semantic` — 27 个测试

### 核心架构

```
SemanticIndex (trait)
       |
  +----+----+
  |         |
TfIdfIndex  (未来: OpenAI/Qdrant Embedding)
  |         |
  +----+----+
       |
SemanticWorkflow (MemoryWorkflow impl)
       |
       v
  EvidenceGraph
```

### 本地 TF-IDF 语义搜索

无需外部 Embedding API，纯本地运行：

- **Token 化**：英文按字母数字分割，中文用字符 n-gram（单字 + bi-gram）
- **TF-IDF 向量化**：Term Frequency - Inverse Document Frequency
- **余弦相似度**：查询与 Episode 的相关度排序
- **命中词项标记**：搜索结果携带 matched_terms 用于调试

### SemanticIndex trait

统一的语义索引接口，不绑定具体实现：

```rust
#[async_trait]
pub trait SemanticIndex: Send + Sync {
    fn name(&self) -> &str;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<ScoredEpisode>, SemanticError>;
    async fn index_episode(&self, episode: &Episode) -> Result<(), SemanticError>;
    async fn index_batch(&self, episodes: &[Episode]) -> Result<(), SemanticError>;
    async fn rebuild_from_store(&self, store: &dyn EpisodeRepository) -> Result<usize, SemanticError>;
    fn doc_count(&self) -> usize;
    async fn clear(&self) -> Result<(), SemanticError>;
}
```

### Runtime 集成

- `MemoryRuntime` 自动注册 `SemanticWorkflow`
- `store_episode()` 和 `auto_store_episode()` 自动写入语义索引
- `MemoryRoute::Semantic` 走 SemanticWorkflow 执行语义搜索
- 降级策略：workflow 不存在时返回空图

## 测试覆盖总览

| Crate | 测试数 |
|---|---|
| `lingshu-memory-episode` | 13 |
| `lingshu-memory-episode-sqlite` | 18 |
| `lingshu-evidence-graph` | 14 |
| `lingshu-memory-workflow` | 10 |
| `lingshu-workflow-memory` | 6 |
| `lingshu-memory-eval` | 32 |
| `lingshu-memory-semantic` | 27 |
| `lingshu-memory-consolidation` | 63 |
| **Memory 总计** | **183** |
| `lingshu-runtime` (memory 集成) | 131 |

## 开发路线图

| Sprint | 里程碑 | 测试 | 状态 |
|---|---|---|---|
| Sprint 1 | Memory Runtime MVP | 43 | ✅ |
| Sprint 2 | SQLite 持久化 + FTS5 | 61 | ✅ |
| Sprint 3-A | Memory Evaluation | 93 | ✅ |
| Sprint 3-B | Memory Consolidation | 156 | ✅ |
| Sprint 3-C | Semantic Memory (TF-IDF) | 183 | ✅ |
| Sprint 4 | Memory Reflection | — | 🔜 |
| Sprint 5 | Agent Memory OS | — | 🔜 |

## 架构演进路线

### 当前（v6）架构

```
Agent Runtime
       |
  MemoryRuntime
       |
├─ MemoryRouter ─→ 规则路由
├─ MemoryWorkflow ─→ Timeline / Semantic / Consolidation
└─ EvidenceGraph ─→ 统一输出 ABI
       |
  Persistent Layer (SQLite / InMemory / TfIdf)
```

### 未来（v7）目标

```
Memory Engine
│
├── Conversation Store
├── Semantic Store (TfIdf + Embedding)
├── Episode Store (SQLite)
├── Graph Store (预留)
│
├── Memory Planner → 策略选择
├── Workflow Runtime → DAG 执行
├── Memory Reasoner → 因果推理
├── Evidence Builder → 证据图构建
├── Memory Evaluator → 自评估反馈
│
└── Consolidation Engine → 短期→长期巩固
```
