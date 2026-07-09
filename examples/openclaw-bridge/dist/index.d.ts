/**
 * openclaw-bridge — MCP 通道网关服务器
 *
 * 将 OpenClaw 风格的消息通道通过 MCP 协议暴露给 lingshu Agent。
 * 支持 Telegram、控制台输出等通道。
 *
 * ## 使用方式
 *
 * ```bash
 * # 启动 (stdio 模式，供 lingshu 启动)
 * node dist/index.js
 *
 * # 或指定配置
 * CHANNEL_CONFIG=./config.json node dist/index.js
 * ```
 *
 * ## MCP 工具列表
 *
 * | 工具 | 描述 |
 * |------|------|
 * | `channels_list` | 列出所有可用通道 |
 * | `channel_send_text` | 发送纯文本消息 |
 * | `channel_send_media` | 发送媒体消息 |
 * | `channel_send_payload` | 发送富文本/互动消息 |
 * | `channel_health` | 检查通道健康状态 |
 */
export {};
