# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
