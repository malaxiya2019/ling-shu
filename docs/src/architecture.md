# 架构概览

Lingshu 采用 **分层微内核** 架构，46 个 crate 按职责分为 7 层：

## 层1: 基础设施

| Crate | 职责 |
|-------|------|
| `core` | 核心类型、LsId、LsError、序列化 |
| `traits` | Plugin、Channel、Memory trait 定义 |
| `config` | 配置加载、环境变量 |

## 层2: 存储与计算

| Crate | 职责 |
|-------|------|
| `database` | SQLite/PostgreSQL、迁移管理 |
| `storage` | 对象存储抽象 |
| `memory` | 记忆系统 (会话/缓冲/向量/图谱) |

## 层3: 通道与通信

| Crate | 职责 |
|-------|------|
| `channel` | Telegram/飞书/QQ/微信 消息通道 |
| `websocket` | WebSocket 实时通信 |
| `federation` | gRPC 联邦通信 |
| `mcp` | MCP 协议 (JSON-RPC 2.0) |

## 层4: 智能与推理

| Crate | 职责 |
|-------|------|
| `backends` | LLM 后端适配器 (27+ 提供商) |
| `llm-router` | LLM 路由 (5 种策略) |
| `orchestrator` | Agent 编排 |
| `runtime` | Agent 运行时管理 |

## 层5: 安全与治理

| Crate | 职责 |
|-------|------|
| `security` | 认证/授权/OAuth2 |
| `tee` | TEE 安全执行 (SGX/TDX) |
| `tenant` | 多租户隔离 |
| `vault` | 密钥管理 |
| `audit` | 审计日志 |

## 层6: 插件与扩展

| Crate | 职责 |
|-------|------|
| `plugin` | 插件管理器、市场 |
| 5 个插件 | beef/watch/rag/code-sandbox/web-search/scheduler |

## 层7: 界面与交互

| Crate | 职责 |
|-------|------|
| `app` | HTTP API 服务器 (FastAPI 风格) |
| `webui` | Yew WASM 管理面板 |
| `desktop` | Tauri 桌面客户端 |
