# 🚀 Lingshu (灵枢) Agent System
[![CI](https://github.com/malaxiya2019/ling-shu/actions/workflows/ci.yml/badge.svg)](https://github.com/malaxiya2019/ling-shu/actions/workflows/ci.yml) 
[![Docker](https://github.com/malaxiya2019/ling-shu/actions/workflows/docker.yml/badge.svg)](https://github.com/malaxiya2019/ling-shu/actions/workflows/docker.yml) 
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT) 
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange)](https://www.rust-lang.org/)

**Lingshu** is a modular agent system framework written in Rust, designed for building production-grade AI agents with strong security, observability, and extensibility.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                   Lingshu Runtime                     │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │Lifecycle │ │ Session  │ │Scheduler │ │ Recovery │ │
│  │  Manager │ │ Manager  │ │          │ │ Manager  │ │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘ │
├──────────────────────────────────────────────────────┤
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │  Event   │ │ Security │ │  Config  │ │Observabil│ │
│  │   Bus    │ │ RBAC+JWT │ │ YAML+Env │ │Tracing+  │ │
│  └──────────┘ └──────────┘ └──────────┘ │  Metrics  │ │
│                                          └──────────┘ │
├──────────────────────────────────────────────────────┤
│  ┌──────────┐ ┌──────────┐ ┌──────────────────────┐  │
│  │ Storage  │ │ Database │ │      Backends         │  │
│  │  Local/  │ │ SQLite+  │ │  ┌────┐ ┌────┐ ┌───┐ │  │
│  │   S3     │ │ Postgres │ │  │LLM │ │Emb │ │VS │ │  │
│  └──────────┘ └──────────┘ │  │    │ │    │ │   │ │  │
│                            │  └────┘ └────┘ └───┘ │  │
│                            └──────────────────────┘  │
└──────────────────────────────────────────────────────┘
```

## Workspace Structure

| Crate | Description |
|-------|-------------|
| `core` | Core types: `LsId`, `LsError`, `LsResult`, `LsContext` |
| `traits` | 14 core trait interfaces (Agent, LLM, Memory, VectorStore, etc.) |
| `runtime` | Lifecycle, Session, Scheduler, Recovery managers |
| `eventbus` | In-memory event bus with publish/subscribe |
| `security` | RBAC+ABAC permissions, Ed25519 service auth, JWT, audit |
| `config` | Layered config (YAML → env → defaults) |
| `storage` | Local filesystem storage implementation |
| `database` | SQLite + PostgreSQL backends with auto-migration |
| `observability` | Tracing (OpenTelemetry), Prometheus metrics, health checks |
| `backends` | LLM (OpenAI, Anthropic, Groq), Embedding, Vector stores |
| `app` | Main binary entry point (REPL + TCP server) |

## Quick Start

```bash
# Build
cargo build

# Run REPL
cargo run -p lingshu

# Run with specific environment
cargo run -p lingshu -- -e prod

# Run as TCP server
cargo run -p lingshu -- --serve --addr 0.0.0.0:8080

# Run with mock LLM (no API key needed)
LS_ENV=dev cargo run -p lingshu
```

## Configuration

Configuration uses YAML files in `config/` with environment variable overrides:

```bash
# Environment
LS_ENV=dev|test|prod

# Config overrides
LS_RUNTIME_MAX_CONCURRENT_TASKS=128
LS_LLM_DEFAULT_MODEL=gpt-4o

# API Keys
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
```

## Features

- **Modular Design**: 10 workspace crates with clean dependency graph
- **Async-First**: Built on Tokio for high concurrency
- **Pluggable Backends**: OpenAI, Anthropic, Groq for LLMs; SQLite, PostgreSQL for storage
- **Production Ready**: Tracing, metrics, health checks, graceful shutdown
- **Security**: RBAC/ABAC permissions, JWT auth, Ed25519 service-to-service auth
- **Observability**: OpenTelemetry integration, Prometheus metrics, structured logging

## License

MIT OR Apache-2.0

## Quick Start

```bash
# Prerequisites
rustup update stable
cargo install cargo-audit cargo-deny  # optional, for security audits

# Build everything
cargo build --all-features

# Run all tests
cargo test --all

# Start HTTP server (REPL mode)
cargo run -p lingshu

# Start HTTP server (API mode)
cargo run -p lingshu -- --addr 0.0.0.0:8080

# Start in production mode
LS_ENV=prod cargo run -p lingshu -- -e prod --addr 0.0.0.0:8080
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

See [.env.example](.env.example) for the full list.

## Project Roadmap

| Phase | Status | Description |
|-------|--------|-------------|
| 0 — Bootstrap | ✅ | Workspace skeleton, crate structure |
| 1 — Core Traits | ✅ | Agent, LLM, Tool, Memory, VectorStore traits |
| 2 — Runtime | ✅ | Lifecycle, Session, Scheduler, Recovery |
| 3 — Security | ✅ | RBAC+ABAC, JWT, Ed25519 auth, audit |
| 4 — Backends | ✅ | OpenAI, Anthropic, Groq, Vector stores |
| 5 — HTTP API | ✅ | REST endpoints, CORS, WebSocket |
| 6 — Test Coverage | ✅ | Runtime, API integration tests |
| 7 — Observability | ✅ | Prometheus metrics, health checks, tracing |
| 8 — CI/CD | ✅ | GitHub Actions, multi-arch Docker |
| 9 — DevX | ✅ | Makefile, .env.example, pre-commit hooks |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check (with subsystem status) |
| GET | `/metrics` | Prometheus metrics |
| GET | `/version` | Version info |
| GET | `/v1/models` | List available models |
| POST | `/v1/chat/completions` | OpenAI-compatible chat completion |
| POST | `/v1/embeddings` | OpenAI-compatible embeddings |
| POST | `/v1/agent/run` | Execute an agent task |
| GET | `/ws` | WebSocket streaming chat |
| GET | `/v1/plugins` | List installed plugins |

## Docker

```bash
# Build
docker build -t lingshu .

# Run with docker-compose (dev)
docker compose up

# Run full stack with PostgreSQL
docker compose --profile full up
```

## Development

```bash
# Use the Makefile for common tasks
make help          # Show available commands
make check         # Check compilation
make test          # Run all tests
make lint          # Check formatting + clippy
make serve         # Start dev server

# Setup pre-commit hooks
git config core.hooksPath .githooks

# Format code before committing
cargo fmt
```

## License

MIT OR Apache-2.0
