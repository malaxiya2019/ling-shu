# 🚀 Lingshu (灵枢) Agent System
[![CI](https://github.com/malaxiya2019/ling-shu/actions/workflows/ci.yml/badge.svg)](https://github.com/malaxiya2019/ling-shu/actions/workflows/ci.yml) 
[![Docker](https://github.com/malaxiya2019/ling-shu/actions/workflows/docker.yml/badge.svg)](https://github.com/malaxiya2019/ling-shu/actions/workflows/docker.yml) 
[![License: MIT](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE) 
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange)](https://www.rust-lang.org/)

**Lingshu** is a modular agent system framework written in Rust, designed for building production-grade AI agents with strong security, observability, and extensibility.

## 安装

### 方式一：下载预编译二进制（推荐）

从 [Releases](https://github.com/malaxiya2019/ling-shu/releases) 下载对应平台的压缩包：

| 平台 | 下载 |
|------|------|
| Linux x86_64 | `lingshu-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 (树莓派等) | `lingshu-aarch64-unknown-linux-gnu.tar.gz` |
| Termux / Android | `lingshu-aarch64-linux-android.tar.gz` |

```bash
# 示例: Linux x86_64
curl -fsSL https://github.com/malaxiya2019/ling-shu/releases/latest/download/lingshu-x86_64-unknown-linux-gnu.tar.gz \
  | tar xz
cd lingshu
cp .env.example .env   # 编辑 .env 填入 API Key
./lingshu
```

### 方式二：一键脚本（自动编译）

```bash
git clone https://github.com/malaxiya2019/ling-shu.git
cd ling-shu
./start.sh
```

> 首次运行会自动检查依赖、引导配置 LLM API Key、编译并启动。
> 后续只需 `./start.sh --quick`。

### 方式三：Docker

```bash
docker pull ghcr.io/malaxiya2019/ling-shu:latest
docker run -p 8080:8080 -e OPENAI_API_KEY=sk-... ghcr.io/malaxiya2019/ling-shu
```

### 系统要求

- **Rust**: 1.81+（仅编译时需要，二进制运行无需）
- **内存**: 编译最低 1GB，运行最低 256MB
- **存储**: 编译 ~5GB，运行 ~200MB
- **系统**: Linux / Termux / macOS（Windows 可通过 WSL）

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         HTTP API (axum)                          │
│  /health  /v1/chat  /v1/agent  /v1/eval  /v1/federation  ...    │
│  /admin (SSR)  /webui (WASM)  /docs (API Docs)                  │
└──────────────────────────┬───────────────────────────────────────┘
                           │
┌──────────────────────────▼───────────────────────────────────────┐
│                      LingshuRuntime                              │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────────────┐  │
│  │Core  │ │Event │ │Agent │ │Memory│ │MCP   │ │Credentials   │  │
│  │Types │ │Bus   │ │Mgr   │ │Mgr   │ │Server│ │Vault         │  │
│  └──────┘ └──────┘ └──────┘ └──────┘ └──────┘ └──────────────┘  │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────────────┐  │
│  │Eval  │ │Fed   │ │Rate  │ │Bill  │ │Audit │ │Knowledge     │  │
│  │Store │ │Feder.│ │Limit │ │System│ │Log   │ │Graph         │  │
│  └──────┘ └──────┘ └──────┘ └──────┘ └──────┘ └──────────────┘  │
└──────────────────────────────────────────────────────────────────┘
                           │
            ┌──────────────┼──────────────┐
            ▼              ▼              ▼
     ┌──────────┐   ┌──────────┐   ┌──────────┐
     │  LLM     │   │  Plugin  │   │  Storage │
     │ Backends │   │ Registry │   │  (Local  │
     │(OpenAI/..)│   │          │   │  + SQLite)│
     └──────────┘   └──────────┘   └──────────┘

┌──────────────────────────────────────────────────────────────────┐
│                       Federation Cluster                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────────┐   │
│  │Discovery │  │  Link    │  │Protocol  │  │  Replication   │   │
│  │(Static/  │  │ (TCP +   │  │(JSON-RPC │  │  (Broadcast/   │   │
│  │ DNS/SRv) │  │  Heart)  │  │  2.0)    │  │  ToLeader)     │   │
│  └──────────┘  └──────────┘  └──────────┘  └────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

## Workspace Structure (44 Crates)

| Crate | Version | Description |
|-------|---------|-------------|
| `core` | v1.x | Core types: `LsId`, `LsError`, `LsResult`, `LsContext` |
| `traits` | v1.x | 14 core trait interfaces (Agent, LLM, Memory, VectorStore, etc.) |
| `runtime` | v1.x | Lifecycle, Session, Scheduler, Recovery managers |
| `eventbus` | v1.x | In-memory event bus with publish/subscribe |
| `security` | v1.x | RBAC+ABAC permissions, Ed25519 service auth, JWT, audit |
| `config` | v1.x | Layered config (YAML → env → defaults) |
| `storage` | v1.x | Local filesystem storage implementation |
| `database` | v1.x | SQLite + PostgreSQL backends with auto-migration |
| `observability` | v1.x | Tracing (OpenTelemetry), Prometheus metrics, health checks |
| `backends` | v1.x | LLM (OpenAI, Anthropic, Groq, Mock), Embedding, Vector stores |
| `plugin` | v1.x | Plugin registration and lifecycle management |
| `orchestrator` | v1.x | Multi-Agent orchestration pipeline |
| `polyglot` | v1.x | 30-language code execution engine |
| `distributed` | v1.x | Distributed runtime types |
| `websocket` | v2.1 | WebSocket connection manager + SSE streaming |
| `memory` | v2.2 | Short-term buffer + long-term vector memory |
| `mcp` | v2.3 | JSON-RPC 2.0 MCP protocol, tool registration |
| `ratelimit` | v2.4 | Token bucket + sliding window rate limiting |
| `billing` | v2.4 | Token-level usage tracking and billing |
| `audit` | v2.4 | Immutable audit log with event tracing |
| `prompt` | v2.4 | Versioned prompt templates with variable injection |
| `multimodal` | v2.5 | Image/audio processing + multimodal RAG |
| `knowledge-graph` | v2.x | Knowledge graph construction and persistence |
| `code-analyzer` | v2.x | Code structure analysis (AST-based) |
| `credentials` | v2.x | Multi-Git-provider credential vault (encrypted) |
| `evaluator` | v2.6 | Agent evaluation framework + regression detection |
| `federation` | v2.7 | Cross-cluster federation communication |
| `webui` | v2.7 | Rust WASM admin panel (Yew CSR) |
| `channel` | v3.4 | Messaging channels: Telegram, Feishu, QQ, WeChat |
| `tee` | v3.3 | TEE (SGX/TDX) secure enclave integration |
| `vault` | v3.3 | Encrypted secrets vault (AES-256-GCM) |
| `tenant` | v3.3 | Multi-tenant isolation and resource quotas |
| `llm-router` | v3.4 | Smart LLM provider routing (latency/cost/fallback) |
| `cache` | v3.4 | Multi-layer cache (memory + disk + Redis) |
| `circuit-breaker` | v3.4 | Circuit breaker for external service resilience |
| `benches` | v3.4 | Criterion benchmarks for key crates |

## Quick Start

### 🚀 一分钟启动（推荐）

```bash
git clone https://github.com/malaxiya2019/ling-shu.git
cd ling-shu
./start.sh
```

第一次运行会自动引导你：
1. 检查 Rust / 系统依赖（缺什么会提示安装命令）
2. 选择 LLM 提供商并输入 API Key
3. 编译并启动服务

> 💡 后续启动只需 `./start.sh --quick`，跳过向导直接启动。
> 环境检查单独运行：`./start.sh --check-env`

### ⚡ 高级用法

```bash
# 首次启动（交互式配置向导）
./start.sh

# 快速启动（跳过检查，直接运行）
./start.sh --quick

# REPL 交互模式（不需要 HTTP 服务）
./start.sh --repl

# 生产模式
./start.sh --env prod --addr 0.0.0.0:8080

# 仅检查环境
./start.sh --check-env

# 国内网络优化（跳过境外站点检测，使用国内镜像）
./start.sh --china

# 集成 OpenClaw MCP 通道网关（需要 Node.js）
./start.sh --with-openclaw
```

### 🔧 手工构建（跳过脚本）

```bash
# Prerequisites
rustup update stable

# Build (默认包含 Telegram / 飞书 / QQ 通道支持)
cargo build --release

# Run all tests (290+ tests, all passing)
cargo test --all --all-features

# Start HTTP API server
cargo run -p lingshu --release -- --addr 0.0.0.0:8080

# Start in production mode
LS_ENV=prod cargo run -p lingshu --release -- -e prod --addr 0.0.0.0:8080

# REPL interactive mode
cargo run -p lingshu --release -- --repl
```

## Configuration

Lingshu uses a layered configuration system (YAML → `.env` → environment variables → defaults).

### 快速配置

复制模板并编辑：

```bash
cp .env.example .env
# 编辑 .env，填入你的 API Key
```

### 核心配置项

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `LS_ENV` | `dev` | 运行环境: `dev` / `test` / `prod` |
| `LLM_PROVIDER` | `openai` | LLM 提供商: `openai` / `deepseek` / `qwen` / `anthropic` / `mock` |
| `OPENAI_API_KEY` | — | OpenAI API Key |
| `DEEPSEEK_API_KEY` | — | DeepSeek API Key (国产推荐) |
| `QWEN_API_KEY` | — | 阿里千问 API Key |
| `ANTHROPIC_API_KEY` | — | Anthropic Claude API Key |
| `LS_LOG_LEVEL` | `debug`(dev) / `info`(prod) | 日志级别 |
| `LINGSHU_TELEGRAM_BOT_TOKEN` | — | Telegram Bot Token (可选) |
| `LINGSHU_FEISHU_APP_ID` | — | 飞书应用 ID (可选) |
| `LINGSHU_QQ_APP_ID` | — | QQ 机器人 ID (可选) |
| `LINGSHU_WECHAT_APP_ID` | — | 微信公众号 App ID (可选) |
| `LINGSHU_WECHAT_APP_SECRET` | — | 微信公众号 App Secret (可选) |
| `LINGSHU_WECHAT_TOKEN` | — | 微信公众号服务器 Token (可选) |

> 📖 完整变量列表见 [.env.example](.env.example)（115 行，含中文注释）

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check (with subsystem status) |
| GET | `/metrics` | Prometheus metrics |
| GET | `/version` | Version info |
| GET | `/docs` | API documentation page |
| GET | `/v1/models` | List available models |
| POST | `/v1/chat/completions` | OpenAI-compatible chat completion |
| POST | `/v1/embeddings` | OpenAI-compatible embeddings |
| POST | `/v1/agent/run` | Execute an agent task |
| GET | `/ws` | WebSocket streaming chat |
| POST | `/v1/eval/run` | Run evaluation suite |
| GET | `/v1/eval/result` | Get latest evaluation result |
| POST | `/v1/eval/regression` | Regression analysis |
| GET | `/v1/federation/status` | Federation cluster status |
| GET | `/v1/federation/nodes` | List online federated nodes |
| POST | `/v1/federation/execute` | Remote execution across cluster |
| POST | `/v1/mcp` | MCP JSON-RPC method call |
| GET | `/v1/graph/{project}` | Knowledge graph query |
| POST | `/v1/credentials` | Credential management |
| GET | `/admin` | Web admin dashboard (server-rendered) |

## Features

- **Modular Design**: 28 workspace crates with clean dependency graph
- **Async-First**: Built on Tokio for high concurrency
- **Pluggable Backends**: OpenAI, Anthropic, Groq for LLMs; SQLite, PostgreSQL for storage
- **Production Ready**: Tracing, metrics, health checks, graceful shutdown
- **Security**: RBAC/ABAC permissions, JWT auth, Ed25519 service-to-service auth, encrypted credential vault
- **Observability**: OpenTelemetry integration, Prometheus metrics, structured logging
- **Agent Evaluation**: Built-in evaluation framework with test suites, metrics, and regression detection
- **Federation**: Cross-cluster agent execution with discovery, heartbeats, and state replication
- **Admin UI**: WASM-based management panel (Yew) + server-rendered fallback
- **Multi-Channel**: Native Telegram / 飞书 / QQ / 微信 (公众号) integration
- **OpenClaw Compatible**: MCP bridge for OpenClaw ecosystem channels
- **Edge-Optimized**: Termux (Android) support, China network mode (`--china`), low-memory operation

## Docker

```bash
# Build
docker build -t lingshu .

# Run with docker-compose (dev)
docker compose up

# Run full stack with PostgreSQL + Redis
docker compose --profile full up

# Run multi-node cluster
docker compose --profile cluster up
```

## One-Click Install

```bash
# Termux (Android) or Ubuntu
bash <(curl -fsSL https://raw.githubusercontent.com/malaxiya2019/ling-shu/main/scripts/install.sh)
```

## Project Roadmap

| Phase | Version | Status | Description |
|-------|---------|--------|-------------|
| 0 — Bootstrap | v1.0 | ✅ | Workspace skeleton, crate structure |
| 1 — Core Traits | v1.0 | ✅ | Agent, LLM, Tool, Memory, VectorStore traits |
| 2 — Runtime | v1.0 | ✅ | Lifecycle, Session, Scheduler, Recovery |
| 3 — Security | v1.0 | ✅ | RBAC+ABAC, JWT, Ed25519 auth, audit |
| 4 — Backends | v1.0 | ✅ | OpenAI, Anthropic, Groq, Vector stores |
| 5 — HTTP API | v1.0 | ✅ | REST endpoints, CORS, WebSocket |
| 6 — Test Coverage | v1.0 | ✅ | Runtime, API integration tests |
| 7 — Observability | v1.0 | ✅ | Prometheus metrics, health checks, tracing |
| 8 — CI/CD | v1.0 | ✅ | GitHub Actions, multi-arch Docker |
| 9 — DevX | v1.0 | ✅ | Makefile, .env.example, pre-commit hooks |
| 10 — Real-time | v2.1 | ✅ | WebSocket + SSE streaming |
| 11 — Memory | v2.2 | ✅ | Session memory, vector retrieval |
| 12 — MCP | v2.3 | ✅ | JSON-RPC MCP protocol, tool system |
| 13 — Platform | v2.4 | ✅ | Rate limit, billing, audit, prompts |
| 14 — Multimodal | v2.5 | ✅ | Image/audio processing, RAG |
| 15 — Evaluation | v2.6 | ✅ | Agent eval framework, regression |
| 16 — Federation | v2.7 | ✅ | Cross-cluster communication, replication |
| 17 — WebUI | v2.7 | ✅ | WASM admin panel, auth, SSR fallback |

## Development

```bash
# Use the Makefile for common tasks
make help          # Show available commands
make check         # Check compilation
make test          # Run all tests
make lint          # Check formatting + clippy
make serve         # Start dev server
make webui-check   # Check WASM compilation
make webui-build   # Build WASM (release)

# Setup pre-commit hooks
git config core.hooksPath .githooks

# Format code before committing
cargo fmt
```

## License

MIT OR Apache-2.0

## 🖥️ Desktop App (Tauri)

Lingshu 可以作为跨平台桌面应用运行（由 Tauri 提供支持）。

```bash
# 安装 Tauri CLI
cargo install tauri-cli

# 构建桌面应用
cd desktop
cargo tauri build

# 开发模式
cargo tauri dev
```

> **系统要求**: Linux 需要 `libwebkit2gtk-4.1-dev`, macOS 需要 Xcode, Windows 需要 WebView2。
> 桌面端构建需在桌面操作系统上进行（不支持 Termux）。

### 桌面端特性

- 🪟 原生窗口体验 (Linux/macOS/Windows)
- 🔔 系统通知 (新消息提醒)
- 📋 剪贴板集成
- 🗂️ 文件对话框 (上传/下载)
- ⚡ 系统托盘 (后台运行)
