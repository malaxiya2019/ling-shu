# 🚀 Lingshu (灵枢) Agent System
[![CI](https://github.com/malaxiya2019/ling-shu/actions/workflows/ci.yml/badge.svg)](https://github.com/malaxiya2019/ling-shu/actions/workflows/ci.yml) 
[![Docker](https://github.com/malaxiya2019/ling-shu/actions/workflows/docker.yml/badge.svg)](https://github.com/malaxiya2019/ling-shu/actions/workflows/docker.yml) 
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT) 
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange)](https://www.rust-lang.org/)

**Lingshu** is a modular agent system framework written in Rust, designed for building production-grade AI agents with strong security, observability, and extensibility.

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

## Workspace Structure (28 Crates)

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

## Quick Start

```bash
# Prerequisites
rustup update stable
cargo install cargo-audit cargo-deny  # optional, for security audits

# Build everything
cargo build --all-features

# Run all tests (290+ tests, all passing)
cargo test --all --all-features

# Start HTTP API server
cargo run -p lingshu -- --addr 0.0.0.0:8080

# Start in production mode
LS_ENV=prod cargo run -p lingshu -- -e prod --addr 0.0.0.0:8080

# REPL interactive mode
cargo run -p lingshu -- --repl
```

## Configuration

Lingshu uses a layered configuration system (YAML → environment variables → defaults):

| Variable | Default | Description |
|----------|---------|-------------|
| `LS_ENV` | `dev` | Runtime environment |
| `LS_LOG_LEVEL` | `debug` (dev) / `info` (prod) | Log level |
| `LS_LLM_PROVIDER` | `mock` | LLM backend to use |
| `LS_ADDR` | `127.0.0.1:8080` | HTTP listen address |
| `LS_MAX_CONCURRENT_TASKS` | `100` | Max concurrent agent tasks |
| `LS_SESSION_TTL_SECONDS` | `3600` | Session expiry in seconds |
| `LS_FEDERATION_PORT` | `9550` | Federation listen port |
| `LS_CLUSTER_NAME` | `lingshu-default` | Federation cluster name |

See [.env.example](.env.example) for the full list.

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
