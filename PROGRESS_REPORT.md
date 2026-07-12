# Lingshu — 项目进度报告

> 生成日期: 2026-07-10

---

## 1. 已完成功能

### v1.x — 核心基础设施 (26 个 Workspace Crates)
| 组件 | 状态 | 说明 |
|------|------|------|
| `core` | ✅ | 核心类型: `LsContext`, `LsError`, `LsId`, `LsResult` |
| `traits` | ✅ | 抽象 Trait: `Llm`, `EventBus`, `ToolProvider` 等 |
| `runtime` | ✅ | 生命周期、调度器、会话管理、Agent 管理 |
| `eventbus` | ✅ | 内存事件总线 (InMemoryEventBus) |
| `security` | ✅ | 服务认证、JWT、密钥管理 |
| `config` | ✅ | 多环境配置加载 (`LsConfig`) |
| `storage` | ✅ | 本地文件存储 |
| `database` | ✅ | 数据库抽象层 (SQLite + PostgreSQL) |
| `observability` | ✅ | 可观测性: 追踪、指标、健康检查 |
| `backends` | ✅ | LLM 后端: OpenAI, Anthropic, Mock |
| `plugin` | ✅ | 插件注册与管理 |
| `orchestrator` | ✅ | Agent 编排流水线 |
| `polyglot` | ✅ | 多语言支持 (30 种语言) |
| `distributed` | ✅ | 分布式基础类型 |

### v2.1 — 实时能力
| 组件 | 状态 | 说明 |
|------|------|------|
| `websocket` | ✅ | WS/SSE 实时流推送、连接管理、心跳 |

### v2.2 — 记忆系统
| 组件 | 状态 | 说明 |
|------|------|------|
| `memory` | ✅ | 会话/缓冲/向量/图谱记忆 |

### v2.3 — MCP 协议
| 组件 | 状态 | 说明 |
|------|------|------|
| `mcp` | ✅ | JSON-RPC 2.0 MCP 协议、工具注册 |

### v2.4 — 平台能力
| 组件 | 状态 | 说明 |
|------|------|------|
| `ratelimit` | ✅ | 多级速率限制 (令牌桶 + 滑动窗口) |
| `billing` | ✅ | 使用量追踪与计费 |
| `audit` | ✅ | 审计日志 |
| `prompt` | ✅ | 提示词注册与管理 |

### v2.5 — 多模态
| 组件 | 状态 | 说明 |
|------|------|------|
| `multimodal` | ✅ | 图像/音频文件处理 + RAG |

### v2.6 — 评测框架 (Evaluator)
| 组件 | 状态 | 说明 |
|------|------|------|
| 测试套件 (`TestSuite`) | ✅ | 用例集合: 名称、分类、元数据 |
| 测试用例 (`TestCase`) | ✅ | 输入/期望/权重/超时 7 种评分类型 |
| 评测运行器 (`EvalRunner`) | ✅ | 并发执行、超时、重试、评分 |
| 指标计算 (`MetricsSummary`) | ✅ | Accuracy, Precision, Recall, F1, P50/P95/P99 延迟 |
| 报告生成 (`ReportGenerator`) | ✅ | JSON + Markdown 格式 |
| 回归检测 (`RegressionDetector`) | ✅ | 基线对比、阈值判定的回归检测 |
| API 端点 | ✅ | `POST /v1/eval/run`, `GET /v1/eval/result`, `POST /v1/eval/regression` |

### v2.7 — 联邦通信 (Federation)
| 组件 | 状态 | 说明 |
|------|------|------|
| 拓扑类型 (`Topology`) | ✅ | Mesh / HubSpoke / Partial |
| 节点发现 (`DiscoveryManager`) | ✅ | StaticDiscovery (种子节点) + DnsDiscovery (SRV) |
| 连接管理 (`LinkManager`) | ✅ | TCP 长度前缀 JSON 协议、Hello 握手、心跳 |
| 消息协议 (`FederationMessage`) | ✅ | Hello/Heartbeat/CapabilityUpdate/RemoteExec/StateReplicate/Error |
| 远程执行 (`RemoteExecutor`) | ✅ | 跨集群 Agent 执行: 能力路由、超时 |
| 状态复制 (`StateReplicator`) | ✅ | Broadcast/ToLeader/Direct 策略 |
| 联邦主入口 (`Federation`) | ✅ | 聚合所有组件的顶层 API |
| API 端点 | ✅ | `GET /v1/federation/status`, `GET /v1/federation/nodes`, `POST /v1/federation/execute` |

### v2.7 — WebUI 管理面板
| 组件 | 状态 | 说明 |
|------|------|------|
| Yew 组件框架 | ✅ | CSR 模式，Yew 0.21 |
| Dashboard 页面 | ✅ | 系统状态、版本、快速链接 |
| Federation 页面 | ✅ | 拓扑图、节点表格、状态卡片 |
| Eval Reports 页面 | ✅ | 评测结果、指标表格 |
| API 客户端 | ✅ | 类型安全的 fetch 封装 |
| 服务端管理面板 | ✅ | 登录认证 + 服务端渲染 Dashboard |
| WASM 构建 | ✅ | `trunk build --release` 已集成到 Dockerfile + CI |

### v2.8 — DevOps & 质量保障
| 组件 | 状态 | 说明 |
|------|------|------|
| Helm Chart | ✅ | K8s 部署 (ConfigMap/Ingress/HPA/联邦多副本) |
| OpenAPI 文档 | ✅ | 自动生成 60+ 端点, 7 标签, Swagger UI |
| 监控 Dashboard | ✅ | WebUI 实时 Metrics 图表 (CPU/Memory/Token) |
| 端到端测试 | ✅ | evaluator + federation 集成测试 (5 个场景) |
| WASM CI 构建 | ✅ | trunk build 集成到 Docker 多阶段构建 + GitHub CI |
| WASM 沙箱修复 | ✅ | wasmtime 平台条件编译, Android 跳过 |

### 附加组件
| 组件 | 状态 | 说明 |
|------|------|------|
| `evaluator` | ✅ | 评测框架 (14 tests passed) |
| `federation` | ✅ | 联邦通信框架 (19 tests passed) |
| `webui` | ✅ | Yew WASM 管理面板 (已 scaffold) |
| `knowledge-graph` | ✅ | 知识图谱构建与持久化 |
| `code-analyzer` | ✅ | 代码结构分析 |
| `credentials` | ✅ | 多 Git 提供商凭证管理 (加密 SQLite) |
| gRPC 服务 | ✅ | tonic + prost 构建, proto 定义 |
| 安装脚本 | ✅ | Termux + Ubuntu 一键安装 |

### v3.0 — SDK & 文档 (Phase 8)
| 组件 | 状态 | 说明 |
|------|------|------|
| mdBook 文档站点 | ✅ | 25 页面完整文档 |
| Python SDK | ✅ | 同步/异步 HTTP 客户端 |
| TypeScript SDK | ✅ | 类型化 HTTP 客户端 |
| WASM Plugin SDK | ✅ | wasmtime 沙箱插件模板 + 构建脚本 |
| Plugin Marketplace | ✅ | 插件索引 + 安装脚本 |
| chidori 集成 | ✅ | durable execution + checkpointing (feature-gated) |
| autoagents 集成 | ✅ | ReAct agent + 结构化工具调用 (feature-gated) |
| loong 集成 | ✅ | 轻量 Agent 基础设施 (feature-gated) |
| llm-router | ✅ | 5 种路由策略 + MetricsCollector |
| 联邦加密与迁移 | ✅ | TLS 加密 + 八卦协议 + Agent 热迁移 |
| 安全增强 | ✅ | OAuth2/OIDC + API Key 轮换 |

### v3.1 — WebUI & API 重构
| 组件 | 状态 | 说明 |
|------|------|------|
| 实时 Metrics JSON 端点 | ✅ | `/v1/metrics` CPU/Memory/Token 实时数据 |
| WebUI Metrics 图表 | ✅ | SVG CPU 仪表盘 + Memory + Token/Request 折线图 |
| 共享 CSS 主题 | ✅ | 346 行 GitHub Dark 主题样式表，移除全部内联样式 |
| API 模块化重构 | ✅ | api.rs → api/ 模块目录 (OpenHands FastAPI 模式) |
| 端到端测试 | ✅ | evaluator + federation 组合集成测试 |

### v3.2 — 性能与可观测性 ✅ (最新完成)
| 组件 | 状态 | 说明 |
|------|------|------|
| gRPC ChatStream 流式推理 | ✅ | Tonic streaming 实时 Token 流 (基于 Llm::invoke_stream) |
| Prometheus AlertManager | ✅ | Helm 集成告警规则 (服务宕机/CPU/内存/Token/错误率/联邦节点/延迟) |
| LLM 缓存层 | ✅ | 新增 `cache/` crate, 支持 In-Memory / Redis / Memcached 后端 |
| Benchmark 仪表盘 | ✅ | WebUI 基准测试结果可视化页面 (8 场景, 4 类别) |

## 2. 代码统计

| 目录 | 源文件 | 代码行 (约) |
|------|--------|-------------|
| `evaluator/` | 7 源文件 | ~1,988 lines |
| `federation/` | 7 源文件 | ~1,988 lines |
| `webui/` | 14 源文件 | ~1,450 lines |
| `cache/` | 3 源文件 | ~380 lines |
| `vault/` | 3 源文件 | ~420 lines |
| `tee/` | 4 源文件 | ~500 lines |
| `tenant/` | 3 源文件 | ~380 lines |
| `app/` | 源文件 (main.rs + api.rs + gRPC + openapi_spec.rs) | ~4,500 lines |
| **总计 (全部 33 crates)** | | **~33,000+ lines** |

## 3. 架构总览

```
┌──────────────────────────────────────────────────────────────────┐
│  HTTP API Server (axum, :8080)  |  gRPC Server (tonic, :50051)   │
│  /v1/chat /v1/agent /v1/eval /v1/federation ...                  │
│  /admin (SSR) /webui (WASM) /docs (API Docs)                     │
└────────────────────────────────┬─────────────────────────────────┘
                                 │
┌────────────────────────────────▼─────────────────────────────────┐
│                        LingshuRuntime                              │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────────────┐  │
│  │Core  │ │Event │ │Agent │ │Memory│ │MCP   │ │Cache  Layer  │  │
│  │Types │ │Bus   │ │Mgr   │ │Mgr   │ │Server│ │(Redis/Mem)   │  │
│  └──────┘ └──────┘ └──────┘ └──────┘ └──────┘ └──────────────┘  │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────────────┐  │
│  │Eval  │ │Fed   │ │Rate  │ │Bill  │ │Audit │ │Metrics +     │  │
│  │Store │ │Feder.│ │Limit │ │System│ │Log   │ │AlertManager  │  │
│  └──────┘ └──────┘ └──────┘ └──────┘ └──────┘ └──────────────┘  │
└──────────────────────────────────────────────────────────────────┘
                                 │
            ┌────────────────────┼────────────────┐
            ▼                    ▼                ▼
     ┌──────────┐       ┌──────────┐       ┌──────────┐
     │  LLM     │       │  Plugin  │       │  Storage │
     │ Backends │       │ Registry │       │  (Local  │
     │(OpenAI/..)│      │          │       │  + SQLite)│
     └──────────┘       └──────────┘       └──────────┘
```


### v4.2.3 — Runtime 指标接入 ✅
| 组件 | 状态 | 说明 |
|------|------|------|
| `ls_agent_count` (Prometheus Gauge) | ✅ | 当前 Agent 数量，ApiHandler handle_agent/handle_runtime 自动更新 |
| `ls_tool_calls_total` (Prometheus Counter) | ✅ | 工具调用累计次数 (labels: tool_name, status)，handle_tool 自动记录 |
| `ls_session_count` (Prometheus Gauge) | ✅ | 当前活跃会话数，handle_session/handle_runtime 自动更新 |
| `RuntimeMetricsCollector` | ✅ | 线程安全的 Prometheus 指标封装，提供 set_agent_count/inc_tool_calls/set_session_count 方法 |
| `RuntimeOtelMetrics` | ✅ | OpenTelemetry Meter API 注册仪表，与 Prometheus 双写 (`#[cfg(feature = "otel")]`) |

### v4.2.4 — RBAC 权限控制 ✅
| 组件 | 状态 | 说明 |
|------|------|------|
| `api::permissions` 模块 | ✅ | 定义 17 个权限常量，格式 `ls.runtime.{domain}.{action}` (agent/session/tool/workflow/runtime 五域) |
| `AuthLayer` 中间件 | ✅ | tower Layer: Bearer Token 提取 → JWT 验证 → PermissionChecker 权限校验 → AuthContext 注入 |
| `route_permission()` | ✅ | 路径→权限映射函数，支持精确匹配 + 前缀匹配，对 `/health` 跳过认证 |
| `AuthContext` | ✅ | 注入 axum 请求扩展，handler 通过 `extract_auth_context()` 提取 (回退匿名) |
| RBAC 测试 | ✅ | 72 个单元测试全部通过，含 `test_health_check_no_auth` (无需认证) + `test_auth_required_for_protected_route` (401 拒绝) |


### v4.2.5 — OmniVoice Studio 语音引擎集成 ✅
| 组件 | 状态 | 说明 |
|------|------|------|
| `start.sh --with-omnivoice` | ✅ | 侧车启动 OmniVoice FastAPI 后端，自动检测依赖，健康检查等待 |
| MCP 服务器配置 | ✅ | `server_launcher.rs` 预置 `omnivoice` MCP server config (python -m backend.mcp_server) |
| 环境变量 | ✅ | `.env.example` 新增 7 个 OmniVoice 配置项 (API URL / TTS/ASR 后端选择) |
| `/v1/audio/speech` (TTS) | ✅ | OpenAI 兼容 TTS 端点，Agent 可直接调用文本→语音 |
| `/v1/audio/transcriptions` (STT) | ✅ | OpenAI 兼容语音识别端点，实现语音→文本 |
| `/ws/tts` (流式 TTS) | ✅ | WebSocket 实时语音流，<100ms 首音延迟 |
| MCP 工具 (`generate_speech`) | ✅ | Agent 通过 MCP 协议调用语音合成 |
| 646 种语言支持 | ✅ | 零样本声音克隆，覆盖全球主要语言 |

### v4.2.6 — 多模态语音 Tool ✅
| 组件 | 状态 | 说明 |
|------|------|------|
| `lingshu-voice` crate | ✅ | Rust HTTP 客户端封装 OmniVoice API |
| `TtsProvider` trait | ✅ | 在 `lingshu-traits` 中定义文本→语音抽象接口 |
| `SttProvider` trait | ✅ | 在 `lingshu-traits` 中定义语音→文本抽象接口 |
| Agent TTS 工具 | ✅ | Agent 可调用 `say` 工具输出语音回复 |
| Agent STT 工具 | ✅ | Agent 可调用 `listen` 工具接收语音输入 |


### v4.2.7 — LTS 稳定版 ✅ (当前)
| 组件 | 状态 | 说明 |
|------|------|------|
| Clippy 零警告 | ✅ | `cargo clippy -D warnings` 全工作空间通过 |
| 代码清理 | ✅ | 修复逻辑 bug (cron_scheduler 布尔表达式)、移除冗余导入、简化表达式 |
| 测试覆盖提升 | ✅ | 补充缺失的单元测试、集成测试，全部 ~500+ 测试通过 |
| 压测与稳定性 | ✅ | 增强 k6 + shell 压测脚本，支持 --long(24h/72h)/--endpoints/--memory 模式 |
| API 文档完善 | ✅ | 完整 REST API 文档 (183行，14类别，100+端点) |
| 部署文档完善 | ✅ | 更新 Docker/K8s/Helm 部署指南 |
| Benchmark 基线 | ✅ | 新增 benchmark_baseline.sh: 启动时间/内存/RPS/延迟 P50/P90/P95/P99/P99.9 |
| LTS 版本发布 | ✅ | 标记 v4.2.7 LTS release (tag v4.2.7-lts, 已推送至 Gitee) |


### v4.3 — Enterprise 🔄 (进行中)
| 组件 | 状态 | 说明 |
|------|------|------|
| Agent 生命周期管理 | ✅ | trait 新增 restart/update_config 方法, AgentManager 实现, API 端点 |
| MCP Server 自动发现 | ✅ | McpDiscovery 引擎 (Static/DNS-SRV/mDNS/HTTP/Manual), API 端点 |
| Token 成本统计 API | ✅ | billing_stats/report/quota/usage 四个端点的 API 定义和路由注册 |
| 路由注册 & 编译验证 | ✅ | 全部新路由注册至 build_router, 工作空间编译通过 |
| API 文档更新 | ✅ | rest-api.md 添加 Agent Lifecycle / Billing / Discovery 端点 |
| 审计日志增强 | 🔄 | Dashboard 页面完善 (进行中) |
| Plugin Marketplace API 增强 | ✅ | market_list/remove_source 端点, RegistrySource source_type/source_url 方法 |
| Billing 内存存储 | ✅ | 全局 LazyLock 内存存储, 按模型/用户统计, 成本估算 |
| 企业 E2E 测试脚本 | ✅ | scripts/enterprise_test.sh (138行, 5类别, 14项测试) |
| 多租户 Dashboard WebUI | ✅ | Tenant 页面 (547行): 组织列表/详情/项目/用户, 侧边栏导航 |
| Web Console | ✅ | 完整管理控制台 (9页面: Dashboard/Agents/Plugins/Billing/Discovery/Tenants/Audit/Federation/Eval) |
| 全量端到端测试增强 | ⏳ | 更多企业场景压测 (规划中) |


## 4. 下一阶段计划

### v3.3 — 企业特性 ✅ (最新完成)
| 组件 | 状态 | 说明 |
|------|------|------|
| 多租户 (Multi-Tenant) | ✅ | 组织/项目/用户三级隔离, tenant/ crate + API 端点 + WebUI |
| 审计仪表盘 (Audit Dashboard) | ✅ | 审计日志实时检索与可视化, WebUI 完整集成 |
| Secrets Vault | ✅ | HashiCorp Vault 集成: KV v2, 动态 Secret, Transit 加解密, Lease 管理 |
| TEE 支持 | ✅ | Intel SGX/TDX 远程证明 + 加密内存区域 + 策略引擎, tee/ crate + API

### v3.4 — 生态系统 ✅
| 组件 | 状态 | 说明 |
|------|------|------|
| OpenHands 集成 | ✅ | api/ 模块化重构: health/metrics/auth/chat/agents/plugins/mcp/federation/eval 9 模块 |
| AutoAgents Orchestrator | ✅ | ReAct agent + 结构化工具调用编排 (feature-gated) |
| chidori Durable Execution | ✅ | checkpointing 持久化恢复 (feature-gated) |
| Plugin 市场 WebUI | ✅ | 在线浏览/搜索/安装/卸载/热加载, 完整 marketplace 集成 |
| Criterion Benchmark | ✅ | 全局基准测试套件: 10+ crate, 15+ 场景, HTML 报告

### v3.5 — 配置与质量提升 ✅
| 组件 | 状态 | 说明 |
|------|------|------|
| start.sh 增强 | ✅ | 691行, 支持 --china/--quick/--repl/--doctor/--update/--with-openclaw 子命令 |
| WebUI 自动构建 | ✅ | start.sh 自动检测 trunk/wasm32 编译 WebUI |
| Web Search 插件 | ✅ | DuckDuckGo 免费搜索, 无需 API Key |
| Tauri 桌面端 | ✅ | Tauri v2 跨平台桌面脚手架 |
| CI/CD 增强 | ✅ | release/bench/coverage/docker/build-deps 五套 CI 流水线 |
| Clippy 全面清理 | ✅ | 分级别 lint 配置, correctness/complexity/perf/style 全开 |

### v3.6 — 通道与向量存储扩展 ✅ (最新完成)
| 组件 | 状态 | 说明 |
|------|------|------|
| Discord 通道插件 | ✅ | 原生 Rust 实现: Bot REST API 发送/接收/回复消息 |
| FastEmbed 本地嵌入 | ✅ | ONNX 运行时本地嵌入, 零外部 API 依赖 (feature-gated: fastembed) |
| Qdrant 向量数据库 | ✅ | 高性能向量搜索后端, 支持集合管理/CRUD/搜索 (feature-gated: vector-store-qdrant) |
| 生产压测脚本 | ✅ | k6 + shell 双重压测: 并发/延迟/资源监控完整覆盖 |
| Criterion 基准测试扩展 | ✅ | 16 场景覆盖 core/cache/memory/security/database/json 等 |
| 通道注册集成 | ✅ | Discord 自动注册至 ChannelRegistry, 按环境变量 DISCORD_BOT_TOKEN 条件激活 |

---
## 5. v4.0 整体评估（里程碑）

| 维度 | 评分 | 说明 |
|------|------|------|
| 架构设计 | ★★★★★ (9.5/10) | Runtime/MCP/Voice/Workflow/Observability 职责清晰 |
| 模块化 | ★★★★★ (9/10) | 功能模块独立 crates，feature-gated |
| 可扩展性 | ★★★★★ (9.5/10) | Provider 接口 (Tts/Stt/LLM/VectorStore) 便于扩展 |
| 生产可用性 | ★★★★☆ (7.5/10) | 架构就绪，仍需真实场景打磨 |
| 文档完整度 | ★★★★☆ (8.5/10) | CHANGELOG + PROGRESS_REPORT + 代码文档 |

### v4.0 已具备能力
- ✅ Runtime 生命周期管理 (LifecycleManager)
- ✅ Agent Factory (LsAgentFactory)
- ✅ Agent Pool (复用池)
- ✅ Agent Pipeline (5 阶段流水线)
- ✅ Memory Pipeline (存储时间线)
- ✅ REST API (axum + JWT RBAC)
- ✅ MCP Tool (7 个 Agent Runtime 工具)
- ✅ Workflow Tool (执行/列表)
- ✅ Voice Tool (TTS/STT)
- ✅ Dashboard (WebUI 状态展示)
- ✅ Metrics (Prometheus + OTel 双写)
- ✅ RBAC (17 权限常量 + AuthLayer)

## 6. 下一阶段路线图 — v4.1 "Production Runtime"

> 目标：从"功能完整"到"生产可靠"，聚焦稳定性、调度、持久化、自动恢复。

### P0 — Agent Task Scheduler（核心）
- [x] Job Queue — 基于内存/SQLite 的任务队列
- [x] Background Worker — 后台任务执行器
- [x] Retry + Timeout — 重试策略 + 超时控制
- [x] Cancel — 任务取消（优雅停止）
- [x] Cron 调度 — 定时触发 Agent

### P0 — Memory 真正落地
- [x] SQLite Store — 持久化内存/会话数据
- [x] Qdrant Vector Search — 语义搜索集成（已有）
- [x] Long-term Memory — 长期记忆（跨会话 Consolidation）
- [x] Session Memory — 会话级上下文管理（已有）+ SQLite 持久化
- [x] Memory Summarization — 记忆摘要/压缩（LLM 驱动）

### P1 — Workflow Engine 增强
- [x] DAG 条件节点 — if/else 分支
- [x] 循环节点 — for/while 循环
- [x] 并行节点 — fan-out/fan-in
- [x] Human Approval — 人工审批节点
- [x] Sub-workflow — 子工作流嵌套

### P1 — 多 Agent 协作
- [x] Planner — 任务规划 Agent
- [x] Executor — 执行 Agent
- [x] Reviewer — 代码/内容审查 Agent
- [x] Router — 任务路由 Agent
- [x] Critic — 批评/改进 Agent
- [x] 结果聚合 — 多 Agent 输出合并

### P2 — 生产能力
- [x] Docker 镜像 — 多阶段构建
- [x] Docker Compose — 一键部署
- [x] Helm Chart — Kubernetes 部署
- [x] CI/CD — 自动测试 + 构建 + 发布
- [x] Benchmark — 压测基准
- [x] Auto Recovery — 崩溃自动恢复

### v4.1 不做的
- ❌ 不新增独立模块/crate
- ❌ 不新增 Provider 接口
- ❌ 不新增 MCP Tool
- ❌ 不新增 WebUI 页面

> 聚焦原则：v4.1 只做一件事——**让 Agent 真正稳定、高效地运行**。
