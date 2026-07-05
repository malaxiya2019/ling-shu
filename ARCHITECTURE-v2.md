# LingShu v2.0 架构设计

> 面向生产环境的下一代 Agent 系统架构

---

## 一、总体架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                       用户层 (User Layer)                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────────┐  │
│  │  Web UI  │  │   CLI    │  │   SDK    │  │  Third-party    │  │
│  │ (React)  │  │ (REPL)   │  │ (Python) │  │  Integrations   │  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └───────┬────────┘  │
├───────┴──────────────┴────────────┴──────────────────┴──────────┤
│                     网关层 (Gateway Layer)                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  API Gateway (Axum + Tower)                              │   │
│  │  ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌────────────┐  │   │
│  │  │ REST    │ │WebSocket │ │ SSE      │ │ gRPC       │  │   │
│  │  │ API v1  │ │ Stream   │ │ Push     │ │ Internal   │  │   │
│  │  └─────────┘ └──────────┘ └──────────┘ └────────────┘  │   │
│  └──────────────────────────────────────────────────────────┘   │
├──────────────────────────────────────────────────────────────────┤
│                    编排层 (Orchestration Layer)                     │
│  ┌──────────┐ ┌───────────┐ ┌──────────┐ ┌──────────────────┐  │
│  │ Agent    │ │ Workflow  │ │ RAG      │ │ Polyglot         │  │
│  │ Orchest. │ │ Engine    │ │ Pipeline │ │ Execution (30L)  │  │
│  └──────────┘ └───────────┘ └──────────┘ └──────────────────┘  │
├──────────────────────────────────────────────────────────────────┤
│                    能力层 (Capability Layer)                       │
│  ┌──────┐ ┌───────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────────┐  │
│  │ LLM  │ │ Tool  │ │Memory│ │Plugin│ │ MCP  │ │ Multi-   │  │
│  │ Hub  │ │System │ │ Mgmt │ │System│ │Proto.│ │ Modal    │  │
│  └──────┘ └───────┘ └──────┘ └──────┘ └──────┘ └──────────┘  │
├──────────────────────────────────────────────────────────────────┤
│                     基础设施层 (Infrastructure)                    │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────────┐  │
│  │Distributed│ │Database │ │ Storage  │ │ Observability    │  │
│  │ Runtime   │ │(PG/Redis)│ │ (S3/Local)│ │(Tracing/Metrics)│  │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

---

## 二、v2.0 新增 Crate 规划

### 2.1 网关层新增

| Crate | 名称 | 说明 |
|-------|------|------|
| `websocket` | LSWs | WebSocket 实时流式传输，支持 Server-Sent Events |
| `ratelimit` | LSRateLimit | 令牌桶 + 滑动窗口限流，支持 per-API-key/per-user |
| `apiversion` | LSApiVersion | API 版本管理 (v1/v2)，向后兼容中间件 |

### 2.2 能力层新增

| Crate | 名称 | 说明 |
|-------|------|------|
| `memory` | LSMemory | 双存储记忆系统：短期 (Buffer) + 长期 (Vector/PG) |
| `mcp` | LSMCP | Model Context Protocol 实现，标准化工具调用 |
| `multimodal` | LSMultiModal | 多模态支持（图像理解、音频转写、文件分析）|
| `evaluator` | LSEvaluator | Agent 评估框架：基准测试、自动评分、回归检测 |
| `prompt` | LSPrompt | 提示词管理：版本化模板、A/B 测试、变量注入 |

### 2.3 平台层新增

| Crate | 名称 | 说明 |
|-------|------|------|
| `billing` | LSBilling | 用量跟踪 + 计费 (token 级粒度) |
| `audit` | LSAudit | 全操作审计日志，不可变事件溯源 |
| `federation` | LSFed | 跨集群联邦通信，Agent 漫游 |
| `webui` | LSWebUI | 内置管理面板 (Rust + WASM, Yew/Leptos) |

### 2.4 基础设施增强

| Crate | 功能升级 |
|-------|---------|
| `backends` | 新增 Gemini、Ollama、vLLM、TogetherAI 后端 |
| `database` | 新增迁移 DSL、连接池优化、读写分离 |
| `distributed` | 新增 Raft 共识、分片、再平衡 |

---

## 三、核心架构改进

### 3.1 Agent 通信协议 — MCP

```
┌─────────┐     MCP Protocol      ┌─────────┐
│ Agent A │ ◄──────────────────►  │ Agent B │
│ (Rust)  │   (JSON-RPC 2.0)     │ (Python)│
└─────────┘                       └─────────┘
       │                               │
       └─────────── MCP Hub ───────────┘
                   │
          ┌────────┴────────┐
          │  Tool Registry  │
          └─────────────────┘
```

MCP 核心能力：
- **工具发现** — 运行时注册/注销
- **能力协商** — 版本握手
- **安全沙箱** — WASM 隔离
- **跨语言调用** — 桥接 Polyglot 引擎

### 3.2 记忆系统分层

```
短期记忆 (Buffer)
  ├── 会话上下文 (窗口滑动)
  ├── Token 用量跟踪
  └── 最近交互缓存

中期记忆 (Working)
  ├── 当前任务状态
  ├── 中间计算结果
  └── Agent 间消息

长期记忆 (Persistent)
  ├── 向量数据库 (pgvector)
  ├── 知识图谱 (Neo4j)
  ├── 用户偏好
  └── 历史决策日志
```

### 3.3 实时流式架构

```
Client                    Server                    LLM Backend
  │                         │                          │
  │── WebSocket Connect ──► │                          │
  │                         │── SSE Stream Start ────► │
  │◄── Stream: Token 1 ─────│◄── Token 1 ─────────────│
  │◄── Stream: Token 2 ─────│◄── Token 2 ─────────────│
  │◄── Stream: Tool Call ───│◄── Tool Call ───────────│
  │── Tool Result ────────► │                          │
  │                         │── Stream Resume ───────► │
  │◄── Stream: Token N ─────│◄── Done ────────────────│
  │◄── Session Complete ────│                          │
```

### 3.4 API 版本化策略

```
/api/v1/chat/completions  — 兼容 OpenAI API
/api/v1/agents            — Agent 管理
/api/v1/workflows         — Workflow 管理

/api/v2/stream            — WebSocket 实时流
/api/v2/memory            — 记忆管理
/api/v2/evaluate          — 评估接口
/api/v2/mcp               — MCP 协议网关
```

---

## 四、分阶段迭代计划

### Phase 1: 实时能力 (v2.1)
1. `websocket` crate — WS 连接管理 + SSE 推送
2. 流式 LLM 响应（逐 token 输出）
3. Agent 执行过程实时推送

### Phase 2: 记忆系统 (v2.2)
1. `memory` crate — 短期/长期记忆
2. 向量检索增强记忆
3. 会话持久化与恢复

### Phase 3: MCP 协议 (v2.3)
1. `mcp` crate — JSON-RPC 2.0 实现
2. 工具发现与注册
3. 跨语言 Agent 通信

### Phase 4: 平台能力 (v2.4)
1. `ratelimit` crate — 限流
2. `billing` crate — 计费
3. `audit` crate — 审计
4. `prompt` crate — 提示词管理

### Phase 5: 多模态 (v2.5)
1. `multimodal` crate — 图像/音频处理
2. 文件分析管道
3. RAG 多模态增强

### Phase 6: 评估与质量 (v2.6)
1. `evaluator` crate — Agent 评测框架
2. 回归测试套件
3. 性能基准

### Phase 7: 联邦与 WebUI (v2.7)
1. `federation` crate — 跨集群通信
2. `webui` crate — 管理面板
3. 集群监控 Dashboard

---

## 五、关键技术决策

| 决策 | 方案 | 理由 |
|------|------|------|
| WebSocket 库 | tokio-tungstenite | 纯 Rust，异步生态 |
| 流式协议 | SSE + WebSocket | SSE 简单，WS 双向 |
| 向量数据库 | pgvector (已有) | 减少运维复杂度 |
| 知识图谱 | Neo4j (可选) | 复杂关系推理 |
| 前端框架 | Leptos (Rust WASM) | 统一技术栈 |
| MCP 序列化 | JSON-RPC 2.0 | 标准协议，互操作 |
| 记忆存储 | PG + Redis | 持久化 + 缓存 |
| 限流算法 | 滑动窗口 | 精确控制 |
| WASM 沙箱 | wasmtime | 安全隔离 |
| CI/CD | GitHub Actions | 已有集成 |

---

## 六、依赖图

```
v2.7  webui ──────────────── federation
                              │
v2.6  evaluator ─────────────┤
                              │
v2.5  multimodal ────────────┤
                              │
v2.4  ratelimit ── billing ── audit ── prompt
                              │
v2.3  mcp ───────────────────┤
                              │
v2.2  memory ────────────────┤
                              │
v2.1  websocket ─────────────┤
                              │
v1.x  [16 crates] ───────────┘
```

每个 Phase 依赖前一个 Phase，但可并发开发非依赖模块。

---

## 七、构建与发布

```yaml
# 发布矩阵
lingshu-v2:
  targets:
    - x86_64-unknown-linux-gnu   # 主发布
    - aarch64-unknown-linux-gnu # ARM (树莓派/Cloud)
    - x86_64-apple-darwin       # macOS
  features:
    default: [websocket, memory, ratelimit]
    full:    [mcp, multimodal, evaluator, federation, webui]
```

---

## 八、从 v1.0 到 v2.0 升级路径

1. **无需中断** — v2 API 与 v1 共存
2. **逐步迁移** — 每个 Phase 独立发布
3. **向后兼容** — 旧 API 路由保留至少两个版本
4. **数据迁移** — PG 迁移脚本自动执行

```
v1.0 ──► v2.1 (WebSocket) ──► v2.2 (+Memory) ──► ... ──► v2.7
         └── API v1 继续运行 ──► API v2 逐步接管 ──► 完全迁移
```
