# LingShu — 分布式 Agent 系统

LingShu 是一个高性能、可扩展的分布式 Agent 系统，提供：

- **多 LLM 路由** — 动态选择、故障转移、成本感知的路由策略
- **联邦通信** — 跨集群 Agent 执行、状态复制、节点发现
- **插件架构** — WASM 沙箱 + 原生插件，动态加载
- **完整可观测性** — OpenTelemetry 追踪、Prometheus 指标、Grafana 面板
- **企业级安全** — JWT/OAuth2/OIDC 认证、RBAC/ABAC 权限、审计日志

## 核心特性

| 特性 | 说明 |
|------|------|
| 🧠 多 LLM 路由 | Priority / Fallback / Latency / Cost / RoundRobin 策略 |
| 🌐 联邦集群 | Mesh / HubSpoke 拓扑，跨集群 Agent 迁移 |
| 🔌 插件系统 | WASM 沙箱 + 原生动态库 |
| 📊 可观测性 | OpenTelemetry + Prometheus + Grafana + Loki |
| 🔒 安全 | JWT / OAuth2 / OIDC + RBAC + ABAC + API Key 轮换 |
| 💾 持久化 | SQLite (dev) + PostgreSQL (prod) + 向量存储 |
| 🧪 评测框架 | 自动化评估 + 回归检测 + 报告生成 |
| 🌍 多语言 | 30+ 语言支持，中英文 WebUI |

## 架构概览

```text
┌────────────────────────────────────────────────────────────┐
│                        API Layer                            │
│   REST (Actix-Web)   gRPC (Tonic)   WebSocket    SSE       │
└──────────────────────┬─────────────────────────────────────┘
                       │
┌──────────────────────▼─────────────────────────────────────┐
│                  Orchestration Layer                        │
│   Router    Orchestrator    Memory    MCP    Federation    │
└──────────────────────┬─────────────────────────────────────┘
                       │
┌──────────────────────▼─────────────────────────────────────┐
│                   Runtime Layer                             │
│   Lifecycle   Scheduler   Session   Recovery   Plugins     │
└──────────────────────┬─────────────────────────────────────┘
                       │
┌──────────────────────▼─────────────────────────────────────┐
│                   Backend Layer                             │
│   OpenAI   Anthropic   Groq   LlamaCpp   LLMKit   Mock    │
└────────────────────────────────────────────────────────────┘
```
