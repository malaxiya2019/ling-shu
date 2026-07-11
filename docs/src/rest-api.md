# REST API 参考

LingShu 提供丰富的 REST API，分为以下类别：

- [管理 & 系统](#系统)--用markdown的二级标题开始
- [Agent 管理](#agent-管理)
- [Chat & LLM](#chat--llm)
- [插件系统](#插件系统)
- [文件 & 多模态](#文件--多模态)
- [联邦 (Federation)](#联邦-federation)
- [评测 (Evaluation)](#评测-evaluation)
- [审计 & 安全](#审计--安全)
- [企业能力](#企业能力)
- [租户 (Multi-Tenant)](#租户-multi-tenant)
- [TEE 安全执行](#tee-安全执行)
- [Vault 密钥管理](#vault-密钥管理)
- [Watch 监控](#watch-监控)
- [MCP 协议](#mcp-协议)

---

## 系统

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查（含子系统状态） |
| GET | `/version` | 版本信息 |
| GET | `/metrics` | Prometheus 指标 |
| GET | `/v1/metrics` | 实时 JSON 指标（CPU/内存/Token） |
| GET | `/v1/models` | 可用模型列表 |
| GET | `/docs` | API 文档页面 |
| GET | `/docs/openapi.json` | OpenAPI 规范 |
| GET | `/docs/swagger` | Swagger UI |

## Agent 管理

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/v1/agent/run` | 执行 Agent 任务 |
| GET | `/v1/agents` | 列出所有 Agent |
| GET | `/v1/agents/:id` | Agent 详情 |
| POST | `/v1/agents/:id/pause` | 暂停 Agent |
| POST | `/v1/agents/:id/resume` | 恢复 Agent |
| POST | `/v1/agents/:id/cancel` | 取消 Agent |

## Chat & LLM

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/v1/chat/completions` | OpenAI 兼容聊天补全 |
| POST | `/v1/chat` | LingShu 原生聊天 |
| POST | `/v1/chat/multimodal` | 多模态聊天（图像+文本） |
| POST | `/v1/embeddings` | OpenAI 兼容嵌入 |
| POST | `/v1/embed` | LingShu 原生嵌入 |
| GET | `/ws` | WebSocket 流式聊天 |
| GET | `/v2/chat/stream` | SSE 流式聊天（v2） |
| GET | `/v2/ws` | WebSocket v2 |
| GET | `/v2/events` | SSE 事件流 |

## 插件系统

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/v1/plugins` | 列出插件 |
| GET | `/v1/plugins/:id` | 插件详情 |
| POST | `/v1/plugins/install` | 安装插件 |
| POST | `/v1/plugins/:id/start` | 启动插件 |
| POST | `/v1/plugins/:id/stop` | 停止插件 |
| POST | `/v1/plugins/:id/uninstall` | 卸载插件 |
| GET | `/v1/plugins/events` | 插件事件流（SSE） |
| POST | `/v1/plugins/hotreload/stop` | 热停止插件 |
| GET | `/v1/plugins/market/search` | 搜索插件市场 |
| POST | `/v1/plugins/market/install` | 从市场安装 |
| POST | `/v1/plugins/market/refresh` | 刷新插件市场缓存 |

## 文件 & 多模态

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/v1/files` | 列出文件 |
| GET | `/v1/files/:id` | 文件详情/下载 |
| POST | `/v1/files/upload` | 上传文件 |
| POST | `/v1/files/analyze` | 文件分析 |
| GET | `/v1/graph/:project/view` | 知识图谱查看 |
| POST | `/v1/credentials/:id/token` | 凭证令牌管理 |
| GET | `/v1/credentials/ui` | 凭证管理 UI |

## 联邦 (Federation)

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/v1/federation/status` | 联邦集群状态 |
| GET | `/v1/federation/nodes` | 在线联邦节点列表 |
| POST | `/v1/federation/execute` | 跨集群远程执行 |

## 评测 (Evaluation)

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/v1/eval/run` | 运行评测套件 |
| GET | `/v1/eval/result` | 获取最新评测结果 |
| POST | `/v1/eval/regression` | 回归分析检测 |

## 审计 & 安全

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/v1/login` | 用户登录 |
| POST | `/v1/logout` | 用户登出 |
| GET | `/api/auth/me` | 当前用户信息 |
| GET | `/v1/audit/logs` | 审计日志检索 |
| POST | `/v1/security/beef/start` | BeEF 安全框架启动 |
| POST | `/v1/security/beef/stop` | BeEF 停止 |
| GET | `/v1/security/beef/status` | BeEF 状态 |
| GET | `/v1/security/beef/hooks` | BeEF Hook 列表 |
| POST | `/v1/security/beef/restart` | BeEF 重启 |
| GET | `/v1/security/beef/hooks` | BeEF Hook 列表 |

## 企业能力

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/v1/projects` | 项目列表 |
| POST | `/v1/mcp` | MCP JSON-RPC 方法调用 |
| GET | `/v1/mcp/tools` | MCP 工具列表 |
| GET | `/v1/mcp/ui` | MCP 管理界面 |

## 租户 (Multi-Tenant)

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/v1/tenant/orgs` | 创建组织 |
| GET | `/v1/tenant/orgs` | 组织列表 |
| GET | `/v1/tenant/orgs/:org_id` | 组织详情 |
| PUT | `/v1/tenant/orgs/:org_id` | 更新组织 |
| POST | `/v1/tenant/orgs/:org_id/projects` | 创建项目 |
| GET | `/v1/tenant/orgs/:org_id/projects` | 项目列表 |
| GET | `/v1/tenant/orgs/:org_id/projects/:project_id` | 项目详情 |
| PUT | `/v1/tenant/orgs/:org_id/projects/:project_id` | 更新项目 |
| POST | `/v1/tenant/orgs/:org_id/users` | 添加用户 |
| GET | `/v1/tenant/orgs/:org_id/users` | 用户列表 |
| PUT | `/v1/tenant/orgs/:org_id/users/:user_id` | 更新用户 |
| GET | `/v1/tenant/stats` | 租户统计 |

## TEE 安全执行

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/v1/tee/health` | TEE 健康检查 |
| POST | `/v1/tee/attest` | 远程证明 |
| POST | `/v1/tee/encrypted-memory` | 创建加密内存 |
| GET | `/v1/tee/encrypted-memory/:id` | 读取加密内存 |
| POST | `/v1/tee/policy` | 设置策略 |

## Vault 密钥管理

| 方法 | 端点 | 说明 |
|------|------|------|
| GET | `/v1/vault/health` | Vault 健康检查 |
| GET | `/v1/vault/secrets` | 密钥列表 |
| GET | `/v1/vault/secrets/*path` | 读取密钥 |
| PUT | `/v1/vault/secrets/*path` | 写入密钥 |
| POST | `/v1/vault/encrypt` | 加密数据 |
| POST | `/v1/vault/decrypt` | 解密数据 |
| GET | `/v1/vault/dynamic-secret/*path` | 获取动态密钥 |
| POST | `/v1/vault/lease/:lease_id/renew` | 续租 |
| POST | `/v1/vault/lease/:lease_id/revoke` | 撤销租约 |

## Watch 监控

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/v1/watch/start` | 启动监控 |
| POST | `/v1/watch/stop` | 停止监控 |
| GET | `/v1/watch/status` | 监控状态 |
| POST | `/v1/watch/ask` | 监控询问 |
| GET | `/v1/watch/video` | 视频流 |
| GET | `/v1/watch/videos` | 视频列表 |
| POST | `/v1/watch/search` | 视频搜索 |

---

> 完整端点详情和请求/响应示例请参见 [OpenAPI 规范](/docs/openapi.json)。
