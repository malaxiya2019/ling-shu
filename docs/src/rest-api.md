# REST API

| 端点 | 方法 | 说明 |
|------|------|------|
| `/v1/health` | GET | 健康检查 |
| `/v1/chat/completions` | POST | 聊天补全 |
| `/v1/agents` | GET | 列出 Agent |
| `/v1/agents/{id}` | GET | Agent 详情 |
| `/v1/plugins` | GET | 列出插件 |
| `/v1/channels/feishu/webhook` | POST | 飞书回调 |
| `/v1/channels/qq/webhook` | POST | QQ 回调 |
| `/v1/metrics` | GET | Prometheus 指标 |
| `/v1/audit` | GET | 审计日志 |
