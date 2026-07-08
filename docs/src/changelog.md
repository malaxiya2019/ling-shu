# Changelog

## 3.0.0 (2026-07-08)

### Features
- Core agent runtime with lifecycle management
- Multi-LLM routing (5 strategies: Priority, Fallback, Latency, Cost, RoundRobin)
- Federation: cross-cluster agent execution with gossip protocol
- WASM plugin sandbox via wasmtime
- Python/TypeScript SDKs
- OpenTelemetry tracing + Prometheus metrics + Grafana dashboard
- Evaluation framework with 7 scoring types
- MCP (Model Context Protocol) support
- Memory system: session, buffer, vector, graph
- OAuth2/OIDC authentication + API key rotation
- LLMKit backend: 27+ providers unified
- LlamaCpp local inference backend

### Changed
- Migrated to 28 workspace crates
- Improved build performance with cargo-chef
- Enhanced Docker multi-stage build

### Fixed
- Plugin wasmtime compilation on non-Termux environments
