# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [4.5.0] - 2026-07-12

### Added
- **Agent 热更新 API** (v4.5):
  - `GET /v1/plugins/hot-reload/status` — 热重载监控器状态（running/capabilities）
  - Web Console 新增 🔄 Hot Reload 页面（状态卡片、启动/停止控制、能力列表）
  - 侧边栏新增 Hot Reload 导航链接
- **测试增强**:
  - `scripts/stress_test.sh`: 新增 审计 端点类别（4端点）+ 热重载类别（1端点）
  - `scripts/enterprise_test.sh`: 新增 7. 热重载 类别（3项）
  - E2E 测试总量扩展至 25 项（17→22→25）

### Changed
- `app/src/api/full.rs`: 新增 hot_reload_status_handler + 路由注册
- `app/src/api/full.rs`: Web Console 新增 renderHotReload() + startHotReload()/stopHotReload()
- `scripts/stress_test.sh`: 新增 endpoints_audit/endpoints_hotreload 类别数组
- `scripts/enterprise_test.sh`: 新增 7. Hot Reload 测试类别

## [4.4.0] - 2026-07-12

### Added
- **审计日志 Dashboard 完善** (v4.4):
  - `GET /v1/audit/stats` — 审计统计（事件类型分布、每日趋势、Top操作者）
  - `GET /v1/audit/entry/:id` — 单条审计详情
  - `GET /v1/audit/export?format=csv|json` — 审计日志 CSV/JSON 导出
  - `POST /v1/audit/archive` — 按时间范围归档旧审计记录
- **Web Console 审计页面增强**:
  - 过滤器面板 (event_type / actor / result / 时间范围)
  - 分页支持 (上一页/下一页/跳页)
  - 统计卡片 (事件类型分布)
  - 导出按钮 (JSON/CSV)
  - 详情模态框（点击行展开完整信息）
- **E2E 测试扩展**:
  - enterprise_test.sh 新增 6. 审计日志 类别（5项测试: query/stats/export-csv/export-json/archive）

### Changed
- `app/src/api/full.rs`: 新增 4 个审计 API handler + 路由注册
- `app/src/api/full.rs`: Web Console renderAudit() 完整重写（+CSS 模态框/分页/字段面板样式）
- `app/src/api/full.rs`: audit_query_handler 修复 total 使用 count() 获取正确总匹配数
- `scripts/enterprise_test.sh`: 从 17 项扩展至 22 项测试

### Fixed
- 修复 audit_query_handler 中分页后 total 计数不正确的问题
- 修复 esc_csv 函数转义逻辑

## [4.3.0] - 2026-07-12

### Added
- **Agent 生命周期管理** (v4.3 Enterprise):
  - Agent trait 新增 `restart()` 和 `update_config()` 方法（带默认实现，不破坏现有代码）
  - AgentManager 新增 `restart()`、`update_config()` 方法
  - API: `POST /v1/agent/:id/restart`, `POST /v1/agent/:id/update`, `DELETE /v1/agent/:id`
- **MCP Server 自动发现** (v4.3 Enterprise):
  - `McpDiscovery` 引擎支持 Static/DNS-SRV/mDNS/HTTP/Manual 五种发现源
  - API: `GET /v1/discovery/servers`, `GET /v1/discovery/health`
- **Token 成本统计 API** (v4.3 Enterprise):
  - `GET /v1/billing/stats` — 全局用量统计
  - `GET /v1/billing/report/:user_id` — 用户成本报告
  - `GET /v1/billing/quota/:user_id` — 用户配额查询
  - `POST /v1/billing/usage` — 记录用量
- **路由系统增强**: 新模块路由 (billing/discovery) 注册至 `build_router`，全工作空间编译通过
- **Plugin Marketplace API 增强**:
  - 新增 `GET /v1/plugins/market/list` — 列出市场本地插件
  - 新增 `DELETE /v1/plugins/market/sources/:source_type` — 移除市场源
  - 增强 `market_sources_handler` 返回真实注册源数据
- **Billing 后端存储**: 全局内存存储 (`std::sync::LazyLock`)，真实跟踪按模型/用户的用量数据
- **企业 E2E 测试**: 新增 `scripts/enterprise_test.sh` (5类别14项测试)

- **多租户 Dashboard WebUI**: 新增 Tenant 页面 (547行): 组织列表/详情/项目/用户视图, 侧边栏导航, 国际化支持
- **Web Console 管理控制台** (v4.3 Enterprise):
  - `GET /admin` 服务端渲染 HTML+JS 控制台 (25202 字符)
  - 9 页面: Dashboard / Agents / Plugins / Billing / MCP Discovery / Tenants / Audit / Federation / Eval
  - 支持 URL 状态恢复 (`/admin?page=xxx`)
  - 暗色主题, 服务端渲染, 零 wasm 依赖
- **企业 E2E 测试增强**: enterprise_test.sh 新增 6 类别 17 项测试

### Changed
- `plugin/src/market.rs`: `RegistrySource` 新增 `source_type()`/`source_url()`; `PluginMarket` 新增 `sources()`/`remove_source()`
- `app/src/api/billing.rs`: 重写为完整内存存储实现，含 2 个单元测试
- `app/src/api/full.rs`: 新增 `market_list_handler`, `market_remove_source_handler` 和路由
- `app/src/api/full.rs`: `build_router` 新增 Agent Lifecycle / Billing / Discovery 路由
- `app/src/api/mod.rs`: 注册 billing 和 discovery 模块
- `traits/src/agent.rs`: `Agent` trait 新增 `restart()` 和 `update_config()`（默认返回 Err(Unsupported)）
- `core/src/error.rs`: 新增 `LsError::Unsupported(String)` 变体
- `runtime/src/agent_manager.rs`: 新增 `restart()`、`update_config()` 方法
- `docs/src/rest-api.md`: 新增 Agent Lifecycle / Billing / Discovery 端点文档

### Fixed
- 修复 `full.rs` 中 `build_router` 缺少 delete method import
- 修复 `billing.rs` 和 `discovery.rs` 中 unused variable warnings

## [4.2.7] - 2026-07-11

### Added
- **压测脚本增强**: `scripts/stress_test.sh` 新增 `--long` (24h/72h 长时间稳定性测试)、`--endpoints` (API 端点覆盖测试)、`--memory` (内存泄漏检测) 三种模式
- **k6 压测脚本增强**: `scripts/stress_test_k6.js` 支持 5 种场景 (standard/endurance/spike/smoke/endpoints)，覆盖 11 个 API 类别
- **Benchmark 基线脚本**: `scripts/benchmark_baseline.sh` 测量启动时间、空闲/负载内存、吞吐量 (RPS)、延迟分布 (P50/P75/P90/P95/P99/P99.9)
- **API 文档完善**: `docs/src/rest-api.md` 重写为 183 行，覆盖 14 类别、100+ 端点
- **部署文档完善**: `docs/src/deployment.md` 更新 Docker/K8s/Helm 部署指南
- **Benchmark 套件**: `benches/src/lingshu_benchmarks.rs` 覆盖 10+ crate、16 场景

### Changed
- `scripts/stress_test.sh`: 从 329 行扩展至 ~550 行，新增长时间稳定性测试、端点覆盖、内存泄漏检测
- `scripts/stress_test_k6.js`: 从单场景扩展至 5 场景，支持阈值配置和多种端点组合
- `PROGRESS_REPORT.md`: v4.2.6 标记为已完成，v4.2.7 LTS 项全部更新

### Fixed
- 全工作空间 Clippy 零警告 (observability/loki, runtime/*, tool/permission, memory/*, backends/*)
- 修复 runtime/cron_scheduler 逻辑 bug (布尔表达式错误)
- 修复多处 redundant import、derivable_impls、new_without_default 警告

## [4.0.0] - 2026-07-10

### Added
- v4.2.3 — Runtime 操作指标接入
  - `lingshu-observability`: 新增 `ls_agent_count` (gauge), `ls_tool_calls_total` (counter), `ls_session_count` (gauge) 三个 Prometheus 指标
  - `RuntimeMetricsCollector`: 结构体封装，提供线程安全的指标更新方法 (agent_count/session_count → gauge, inc_tool_calls → counter)
  - `RuntimeOtelMetrics`: OpenTelemetry Meter API 注册对应仪表 (`ls.runtime.agent_count` gauge, `ls.runtime.tool_calls` counter, `ls.runtime.session_count` gauge)
  - `lingshu-runtime` ApiHandler: 内置 `RuntimeMetricsCollector` + 可选 `RuntimeOtelMetrics`，在 handle_agent/handle_session/handle_tool/handle_runtime 中自动记录指标
- v4.2.4 — RBAC 权限控制
  - `lingshu-runtime`: `api::permissions` 模块定义每个 API 端点权限常量 (格式: `ls.runtime.{domain}.{action}`)
  - `AuthLayer` (tower Layer 中间件): JWT Bearer Token 验证 + PermissionChecker 权限校验
  - `AuthContext`: 注入到 axum 请求扩展，handler 可提取认证上下文
  - 路由级权限映射: `route_permission()` 函数做路径→权限匹配，对 `/health` 跳过认证
  - HTTP 状态码: 401 (无令牌/验证失败) / 403 (权限不足)

  - ling-shu MCP 客户端: `server_launcher.rs` 添加 `omnivoice` 默认 MCP 服务器配置
  - `start.sh`: 新增 `--with-omnivoice` 标志，自动侧车启动 OmniVoice 后端 (FastAPI)
  - `.env.example`: 新增 7 个 OmniVoice 环境变量 (OMNIVOICE_API_URL / TTS_BACKEND / ASR_BACKEND 等)

### Changed
- `runtime/Cargo.toml`: `tower` 依赖从 0.4 升级到 0.5 (匹配 axum 0.7), 新增 `otel` feature (→ lingshu-observability/otel)
- `observability/src/otel.rs`: `.init()` → `.build()` 适配 opentelemetry 0.27 API
## [3.6.0] - 2026-07-10

### Added
- Discord 通道插件 (`channel/src/discord.rs`): 原生 Rust 实现, 支持 Bot REST API 发送/接收消息
- FastEmbed 本地嵌入模型 (`backends/src/embedding_fastembed.rs`): 基于 ONNX 运行时, 零 API 依赖
- Qdrant 高性能向量数据库 (`backends/src/vector_store_qdrant.rs`): 高并发向量搜索后端
- 生产压测脚本 (`scripts/stress_test.sh` + `scripts/stress_test_k6.js`): 并发/延迟/资源监控
- Criterion 基准测试扩展: 16 个场景覆盖 core/cache/memory/security/database/json 等组件
- 基准测试错误修复: 变量名冲突、类型名不匹配、API 签名纠正 (18 处编译错误修复)
- `Makefile` 新增目标: `docs`, `docs-serve`, `docs-clean`, `lint`, `lint-fix`, `check-all`

### Changed
- `app/Cargo.toml`: 默认 features 新增 `discord` 通道支持
- `channel/Cargo.toml`: 新增 `discord` feature (依赖 reqwest)
- `backends/Cargo.toml`: 新增 `fastembed` + `vector-store-qdrant` feature
- `backends/src/lib.rs`: 注册 FastEmbed 和 Qdrant 模块导出
- `channel/src/lib.rs`: 注册 Discord 通道模块

### Fixed
- `benches/src/lingshu_benchmarks.rs`: 修复 18 处编译错误 (变量名冲突去除、类型名纠正、API 签名适配)
- `app/src/api/full.rs`: 移除已删除的 `otel_guard` 字段引用, 添加 `channel_registry` 字段
- `plugins/rag-plugin/src/lib.rs`: 移除与 beef-plugin 冲突的 `create_plugin` 符号导出
- `channel/src/wechat.rs`: 修复 XML 解析未处理 CDATA 导致测试失败的问题
- `channel/src/wechat.rs`: 新增 `strip_cdata` 辅助函数, 正确剥离 CDATA 包裹内容

## [3.5.0] - 2026-07-09

### Added
- `start.sh --china` 标志: 跳过境外检测 + USTC cargo 镜像提示 + 国内站点加速
- `start.sh --with-openclaw` 标志: 自动构建并启动 openclaw MCP HTTP Bridge
- `start.sh update` 增强: 自动 `cargo install` + 版本 changelog 展示
- WebUI 自动构建: `start.sh` 自动检测 trunk/wasm32 并编译 WebUI
- openclaw MCP HTTP Bridge (`examples/openclaw-bridge/`): 支持 HTTP 模式 (HTTP_PORT 环境变量)
- web-search 插件 (`plugins/web-search-plugin/`): DuckDuckGo 免费搜索引擎, 无需 API Key
- Tauri 桌面端脚手架 (`desktop/`): Tauri v2 框架, 跨平台桌面支持
- CI 增强: release.yml (x86_64 / aarch64 / Termux 三平台打包), bench.yml (Criterion), coverage.yml, docker.yml, build-deps.yml

### Changed
- `start.sh` 升级至 691 行, 支持 7 个子命令: --check-env/--quick/--repl/--doctor/--update/--china/--with-openclaw
- CI clippy 配置优化: 分级别 lint (-D correctness, -W style/complexity/perf)
- README 完整更新: 44 crate 架构表, 新命令行参数, WeChat 配置说明, OpenClaw 集成文档, 桌面端构建指引
- `.gitignore` 更新: 排除 openclaw-bridge 构建产物

### Fixed
- `code-sandbox-plugin`: 消除 `dead_code` 警告 (self.status 和 self.created_at 改为使用)
- SQLite 迁移 `005_checkpoint.sql` 注册修复 (database/src/sqlite.rs)

## [3.4.0] - 2026-07-09

### Added
- 一键启动脚本 `start.sh`: 跨平台依赖检测 + 配置向导 + 编译 + 启动
- `.env.example` 配置模板: 115 行含中文注释, 覆盖 LLM/通道/数据库/可观测性
- 通道支持: Telegram / 飞书 / QQ 默认启用
- `dotenvy` 自动加载 `.env` 文件
- CONTRIBUTING.md, SECURITY.md, CHANGELOG.md 工程文档
- LICENSE (MIT + Apache-2.0)
- GitHub Release 自动打包 workflow (x86_64 / aarch64 / Termux)

### Changed
- `app/Cargo.toml`: 默认 features 包含 `["telegram", "feishu", "qq"]`
- 重构 `channel/src/router.rs` 测试模块, 修复 brace 失衡导致编译失败
- 修复 doc test 中 `db` 未定义问题
- 消除全部 34 个编译警告

### Fixed
- `channel/src/router.rs`: 多余 `}` 导致 `mod tests` 提前关闭
- 文档测试引用未定义变量 `db`

## [3.3.0] - 2026-07-04

### Added
- 多租户隔离 (Tenant)
- 审计仪表盘 (Audit Dashboard)
- Secrets Vault (HashiCorp Vault 集成)
- TEE 支持 (Intel SGX/TDX)

## [3.2.0] - 2026-06-28

### Added
- gRPC ChatStream 流式推理
- Prometheus AlertManager 集成
- LLM 缓存层 (In-Memory / Redis / Memcached)
- Benchmark 仪表盘

## [3.1.0] - 2026-06-20

### Added
- Yew WebUI 管理面板
- API 模块化重构 (OpenHands FastAPI 模式)
- 实时 Metrics JSON 端点

## [3.0.0] - 2026-06-10

### Added
- Python SDK
- TypeScript SDK
- WASM Plugin SDK
- mdBook 文档站点 (25 页面)
- Plugin Marketplace
- llm-router (5 种路由策略)
- 联邦加密与 TLS
- OAuth2/OIDC 安全增强

## [2.0.0] - 2026-05-15

### Added
- 核心 26 crate workspace 架构
- LLM 后端: OpenAI, Anthropic, DeepSeek, Qwen 等
- Agent 编排流水线
- 记忆系统 (会话/缓冲/向量/图谱)
- MCP 协议 (JSON-RPC 2.0)
- 联邦通信 (Federation)
- 评测框架 (Evaluator)

## [1.0.0] - 2026-04-01

### Added
- 项目初始化: 核心类型, Traits, 运行时
- SQLite + PostgreSQL 数据库支持
- 事件总线, 安全认证, 配置加载
