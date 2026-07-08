# Architecture

> Overview of the LingShu system architecture.

## High-Level Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                        API Layer                              │
│   REST (Actix-Web)   gRPC (Tonic)   WebSocket    SSE        │
└──────────────────────┬───────────────────────────────────────┘
                       │
┌──────────────────────▼───────────────────────────────────────┐
│                  Orchestration Layer                          │
│   Router    Orchestrator    Memory    MCP    Federation      │
└──────────────────────┬───────────────────────────────────────┘
                       │
┌──────────────────────▼───────────────────────────────────────┐
│                   Runtime Layer                               │
│   Lifecycle   Scheduler   Session   Recovery   Plugins       │
└──────────────────────┬───────────────────────────────────────┘
                       │
┌──────────────────────▼───────────────────────────────────────┐
│                   Backend Layer                               │
│   OpenAI   Anthropic   Groq   LlamaCpp   LLMKit   Mock      │
└──────────────────────────────────────────────────────────────┘
```

## Layer Description

### API Layer
- REST API via Actix-Web
- gRPC services via Tonic
- WebSocket connections for streaming
- SSE for server-sent events

### Orchestration Layer
- **Router**: Multi-LLM routing with 5 strategies
- **Orchestrator**: Agent pipeline orchestration
- **Memory**: Session/buffer/vector/graph memory
- **MCP**: Model Context Protocol (JSON-RPC 2.0)
- **Federation**: Cross-cluster agent communication

### Runtime Layer
- **Lifecycle**: Agent lifecycle management
- **Scheduler**: Task scheduling and execution
- **Session**: Session state management
- **Recovery**: Checkpoint-based recovery
- **Plugins**: WASM + native plugin system

### Backend Layer
- Multi-LLM backend support
- Unified API through trait abstraction
- Automatic failover and load balancing
