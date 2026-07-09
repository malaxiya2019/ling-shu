# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
