# openclaw-bridge — MCP 通道网关

将 lingshu Agent 的消息通道能力通过 **MCP 协议**暴露给外部应用。

## 架构

```
lingshu Agent ──MCP Client──► openclaw-bridge (MCP Server) ──► Telegram / Console / ...
                                    │
                            ┌───────┴───────┐
                            │  config.json  │
                            └───────────────┘
```

## 快速开始

```bash
cd examples/openclaw-bridge

# 安装依赖
npm install

# 使用默认控制台通道启动
npm start
```

## 配置

复制 `config.example.json` 为 `config.json` 并按需修改：

```json
{
  "channels": [
    {
      "id": "telegram-main",
      "label": "Telegram Bot",
      "platform": "telegram",
      "options": {
        "token": "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"
      }
    }
  ]
}
```

或通过环境变量指定配置路径：
```bash
CHANNEL_CONFIG=/path/to/config.json npm start
```

## MCP 工具

| 工具 | 描述 |
|------|------|
| `channels_list` | 列出所有可用通道 |
| `channel_send_text` | 发送纯文本消息 |
| `channel_send_media` | 发送媒体消息 |
| `channel_send_payload` | 发送富文本消息 |
| `channel_health` | 检查通道健康状态 |

## 与 lingshu 集成

在 lingshu 中配置 MCP 服务器连接到 openclaw-bridge：

```json
{
  "mcp_servers": {
    "openclaw-bridge": {
      "command": "node",
      "args": ["/path/to/openclaw-bridge/dist/index.js"],
      "env": {
        "CHANNEL_CONFIG": "/path/to/config.json"
      }
    }
  }
}
```
