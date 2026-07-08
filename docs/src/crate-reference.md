# Crate Reference

| Crate | Path | Description |
|-------|------|-------------|
| core | `core/` | Core types: LsContext, LsError, LsId |
| traits | `traits/` | Abstract traits: Llm, EventBus, ToolProvider |
| runtime | `runtime/` | Lifecycle, scheduler, session management |
| eventbus | `eventbus/` | InMemoryEventBus implementation |
| security | `security/` | JWT, OAuth2, RBAC, API key rotation |
| config | `config/` | Multi-environment config loading |
| storage | `storage/` | Local file storage |
| database | `database/` | SQLite + PostgreSQL (SeaORM) |
| observability | `observability/` | Tracing, metrics, health checks |
| backends | `backends/` | LLM backends: OpenAI, Anthropic, Mock |
| plugin | `plugin/` | Plugin registry and management |
| orchestrator | `orchestrator/` | Agent pipeline orchestration |
| polyglot | `polyglot/` | Multi-language support (30 langs) |
| distributed | `distributed/` | Distributed base types |
| websocket | `websocket/` | WS/SSE real-time streaming |
| memory | `memory/` | Session/buffer/vector/graph memory |
| mcp | `mcp/` | JSON-RPC 2.0 MCP protocol |
| multimodal | `multimodal/` | Image/audio processing + RAG |
| ratelimit | `ratelimit/` | Token bucket + sliding window |
| billing | `billing/` | Usage tracking and billing |
| audit | `audit/` | Audit logging |
| prompt | `prompt/` | Prompt registry and management |
| code-analyzer | `code-analyzer/` | Code analysis tools |
| knowledge-graph | `knowledge-graph/` | Knowledge graph support |
| credentials | `credentials/` | Credential management |
| evaluator | `evaluator/` | Evaluation framework |
| federation | `federation/` | Cross-cluster communication |
| llm-router | `llm-router/` | Multi-LLM routing (5 strategies) |
| webui | `webui/` | Yew-based WebAssembly UI |
| app | `app/` | Application entry point |
