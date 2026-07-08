# Architecture Deep Dive

## Crate Dependency Graph

```text
core ──► traits ──► runtime ──► orchestrator ──► app
  │                    │              │
  │                    ▼              ▼
  ├──► eventbus    database      federation
  ├──► config      storage       mcp
  ├──► security    observability memory
  ├──► backends    plugin        evaluator
  └──► polyglot    distributed   llm-router
```

## Core Trait System

The system is built on Rust trait abstractions:

- `Llm` — LLM backend interface
- `EventBus` — Event publish/subscribe
- `ToolProvider` — Tool registration and execution
- `MemoryStore` — Memory persistence
- `PluginRuntime` — Plugin lifecycle
- `FederationNode` — Cluster communication

## Data Flow

1. **Request arrives** at API layer (REST/gRPC/WS)
2. **Authentication** via security layer
3. **Rate limiting** via ratelimit crate
4. **Router** selects LLM backend (5 strategies)
5. **Orchestrator** executes agent pipeline
6. **Runtime** manages lifecycle/scheduling
7. **Memory** stores context
8. **Response** flows back through the layers
