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

// ── 导入 ───────────────────────────────────────────

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  ToolSchema,
} from "@modelcontextprotocol/sdk/types.js";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// ── 类型定义 ───────────────────────────────────────

/** 通道配置 */
interface ChannelConfig {
  /** 通道唯一标识 */
  id: string;
  /** 显示名称 */
  label: string;
  /** 平台类型: "telegram" | "console" | "wechat" | "discord" */
  platform: string;
  /** 平台专用配置 */
  options: Record<string, string>;
}

/** 通道插件接口 */
interface ChannelPlugin {
  readonly id: string;
  readonly label: string;
  readonly platform: string;

  /** 发送纯文本 */
  sendText(to: string, text: string, replyTo?: string): Promise<SendReceipt>;
  /** 发送媒体 */
  sendMedia(to: string, mediaUrl: string, text?: string): Promise<SendReceipt>;
  /** 发送富文本载荷 */
  sendPayload(to: string, payload: ReplyPayload): Promise<SendReceipt>;
  /** 健康检查 */
  healthCheck(): Promise<HealthStatus>;
}

/** 发送回执 */
interface SendReceipt {
  success: boolean;
  messageId?: string;
  error?: string;
  timestamp: number;
}

/** 回复载荷 */
interface ReplyPayload {
  text?: string;
  mediaUrls?: string[];
  replyToId?: string;
  isError?: boolean;
}

/** 健康状态 */
interface HealthStatus {
  healthy: boolean;
  latencyMs?: number;
  error?: string;
}

// ── 通道实现 ───────────────────────────────────────

/**
 * 控制台通道 — 仅打印到 stdout
 * 用于测试和调试
 */
class ConsoleChannel implements ChannelPlugin {
  readonly id: string;
  readonly label: string;
  readonly platform = "console";

  constructor(config: ChannelConfig) {
    this.id = config.id;
    this.label = config.label;
  }

  async sendText(to: string, text: string, replyTo?: string): Promise<SendReceipt> {
    console.log(`[Console:${this.id}] → ${to}: ${text}${replyTo ? ` (回复:${replyTo})` : ""}`);
    return {
      success: true,
      messageId: `console-${Date.now()}`,
      timestamp: Date.now(),
    };
  }

  async sendMedia(to: string, mediaUrl: string, text?: string): Promise<SendReceipt> {
    console.log(`[Console:${this.id}] → ${to}: 📎 ${mediaUrl}${text ? ` (${text})` : ""}`);
    return {
      success: true,
      messageId: `console-${Date.now()}`,
      timestamp: Date.now(),
    };
  }

  async sendPayload(to: string, payload: ReplyPayload): Promise<SendReceipt> {
    console.log(`[Console:${this.id}] → ${to}: 📦`, JSON.stringify(payload));
    return {
      success: true,
      messageId: `console-${Date.now()}`,
      timestamp: Date.now(),
    };
  }

  async healthCheck(): Promise<HealthStatus> {
    return { healthy: true, latencyMs: 0 };
  }
}

/**
 * Telegram 通道 — 通过 Bot API 发送消息
 * 需要配置 TELEGRAM_BOT_TOKEN
 */
class TelegramChannel implements ChannelPlugin {
  readonly id: string;
  readonly label: string;
  readonly platform = "telegram";
  private botToken: string;
  private apiBase: string;

  constructor(config: ChannelConfig) {
    this.id = config.id;
    this.label = config.label;
    this.botToken = config.options.token || process.env.TELEGRAM_BOT_TOKEN || "";
    this.apiBase = config.options.apiBase || `https://api.telegram.org/bot${this.botToken}`;

    if (!this.botToken) {
      console.warn(`[WARN] Telegram channel "${this.id}" has no bot token configured`);
    }
  }

  private async callApi(method: string, params: Record<string, unknown>): Promise<SendReceipt> {
    try {
      const url = `${this.apiBase}/${method}`;
      const resp = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(params),
      });
      const data = await resp.json() as { ok: boolean; result?: { message_id: number }; description?: string };
      if (!data.ok) {
        return { success: false, error: data.description || "Telegram API error", timestamp: Date.now() };
      }
      return {
        success: true,
        messageId: String(data.result?.message_id ?? ""),
        timestamp: Date.now(),
      };
    } catch (err) {
      return { success: false, error: String(err), timestamp: Date.now() };
    }
  }

  async sendText(to: string, text: string, replyTo?: string): Promise<SendReceipt> {
    const params: Record<string, unknown> = {
      chat_id: to,
      text,
      parse_mode: "HTML",
    };
    if (replyTo) params.reply_to_message_id = Number(replyTo);
    return this.callApi("sendMessage", params);
  }

  async sendMedia(to: string, mediaUrl: string, text?: string): Promise<SendReceipt> {
    const isPhoto = /\.(png|jpg|jpeg|webp|gif)$/i.test(mediaUrl);
    const method = isPhoto ? "sendPhoto" : "sendDocument";
    const params: Record<string, unknown> = {
      chat_id: to,
      [isPhoto ? "photo" : "document"]: mediaUrl,
    };
    if (text) params.caption = text;
    return this.callApi(method, params);
  }

  async sendPayload(to: string, payload: ReplyPayload): Promise<SendReceipt> {
    // 富文本降级：有多媒体则发送媒体，否则发文本
    if (payload.mediaUrls?.length) {
      return this.sendMedia(to, payload.mediaUrls[0], payload.text);
    }
    return this.sendText(to, payload.text || "", payload.replyToId);
  }

  async healthCheck(): Promise<HealthStatus> {
    const start = Date.now();
    try {
      const resp = await fetch(`${this.apiBase}/getMe`);
      const data = await resp.json() as { ok: boolean };
      return {
        healthy: data.ok,
        latencyMs: Date.now() - start,
        error: data.ok ? undefined : "Telegram bot not found",
      };
    } catch (err) {
      return { healthy: false, error: String(err), latencyMs: Date.now() - start };
    }
  }
}

// ── 配置加载 ───────────────────────────────────────

function loadConfig(): ChannelConfig[] {
  const configPath = process.env.CHANNEL_CONFIG
    || path.join(process.cwd(), "config.json");

  try {
    const raw = fs.readFileSync(configPath, "utf-8");
    const parsed = JSON.parse(raw);
    const channels = Array.isArray(parsed) ? parsed : parsed.channels;
    if (!Array.isArray(channels)) {
      throw new Error("Config must contain a 'channels' array or be an array");
    }
    return channels;
  } catch (err) {
    console.warn(`[WARN] No config file found at ${configPath}, using default console channel`);
    return [
      { id: "console", label: "控制台输出", platform: "console", options: {} },
    ];
  }
}


/**
 * 飞书 (Feishu/Lark) 通道 — 通过开放平台 API 发送消息
 * 需要配置 FEISHU_APP_ID 和 FEISHU_APP_SECRET
 */
class FeishuChannel implements ChannelPlugin {
  readonly id: string;
  readonly label: string;
  readonly platform = "feishu";
  private appId: string;
  private appSecret: string;
  private apiBase: string;
  private tokenCache: { token: string; expiresAt: number } | null = null;

  constructor(config: ChannelConfig) {
    this.id = config.id;
    this.label = config.label;
    this.appId = config.options.appId || process.env.FEISHU_APP_ID || "";
    this.appSecret = config.options.appSecret || process.env.FEISHU_APP_SECRET || "";
    this.apiBase = config.options.apiBase || "https://open.feishu.cn";

    if (!this.appId || !this.appSecret) {
      console.warn(`[WARN] Feishu channel "${this.id}" has no app credentials configured`);
    }
  }

  /** 获取 tenant_access_token（自动缓存） */
  private async getToken(): Promise<string> {
    const now = Math.floor(Date.now() / 1000);
    if (this.tokenCache && now < this.tokenCache.expiresAt - 60) {
      return this.tokenCache.token;
    }

    const resp = await fetch(`${this.apiBase}/open-apis/auth/v3/tenant_access_token/internal`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ app_id: this.appId, app_secret: this.appSecret }),
    });
    const data = await resp.json() as {
      code: number; msg?: string;
      tenant_access_token?: string; expire?: number
    };
    if (data.code !== 0) {
      throw new Error(`Feishu auth error [${data.code}]: ${data.msg}`);
    }
    if (!data.tenant_access_token) {
      throw new Error("Feishu auth response missing tenant_access_token");
    }
    this.tokenCache = {
      token: data.tenant_access_token,
      expiresAt: now + (data.expire || 7200),
    };
    return data.tenant_access_token;
  }

  /** 调用飞书 Open API */
  private async callApi(
    path: string, body?: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const token = await this.getToken();
    const url = `${this.apiBase}${path}`;
    const resp = await fetch(url, {
      method: "POST",
      headers: {
        "Authorization": `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    const data = await resp.json() as { code: number; msg?: string; data?: Record<string, unknown> };
    if (data.code !== 0) {
      throw new Error(`Feishu API error [${data.code}]: ${data.msg}`);
    }
    return data.data ?? {};
  }

  async sendText(to: string, text: string, replyTo?: string): Promise<SendReceipt> {
    const params: Record<string, unknown> = {
      receive_id: to,
      msg_type: "text",
      content: JSON.stringify({ text }),
    };
    const result = await this.callApi(
      `/open-apis/im/v1/messages?receive_id_type=open_id`, params
    );
    return {
      success: true,
      messageId: (result.message_id as string) ?? "",
      timestamp: Date.now(),
    };
  }

  async sendMedia(to: string, mediaUrl: string, text?: string): Promise<SendReceipt> {
    // 飞书图片需要先上传获取 image_key，简化版发送链接文本
    const content = JSON.stringify({
      text: `📎 ${mediaUrl}${text ? `\n${text}` : ""}`,
    });
    return this.sendText(to, content);
  }

  async sendPayload(to: string, payload: ReplyPayload): Promise<SendReceipt> {
    let text = payload.text || "";
    if (payload.mediaUrls?.length) {
      text += "\n" + payload.mediaUrls.map((u, i) => `📎 [${i}](${u})`).join("\n");
    }
    if (payload.isError) {
      text = `❌ ${text}`;
    }
    return this.sendText(to, text);
  }

  async healthCheck(): Promise<HealthStatus> {
    const start = Date.now();
    try {
      await this.getToken();
      return { healthy: true, latencyMs: Date.now() - start };
    } catch (err) {
      return { healthy: false, error: String(err), latencyMs: Date.now() - start };
    }
  }
}

/**
 * QQ 通道 — 通过 QQ 官方机器人平台 API 发送消息
 * 需要配置 QQ_APP_ID 和 QQ_BOT_TOKEN
 */
class QqChannel implements ChannelPlugin {
  readonly id: string;
  readonly label: string;
  readonly platform = "qq";
  private appId: string;
  private botToken: string;
  private apiBase: string;

  constructor(config: ChannelConfig) {
    this.id = config.id;
    this.label = config.label;
    this.appId = config.options.appId || process.env.QQ_APP_ID || "";
    this.botToken = config.options.botToken || process.env.QQ_BOT_TOKEN || "";
    this.apiBase = config.options.apiBase || "https://api.sgroup.qq.com";

    if (!this.appId || !this.botToken) {
      console.warn(`[WARN] QQ channel "${this.id}" has no credentials configured`);
    }
  }

  private authHeader(): string {
    return `Bot ${this.appId}.${this.botToken}`;
  }

  private isGroupTarget(id: string): boolean {
    return id.startsWith("AO_") || id.startsWith("group_");
  }

  private async callApi(
    path: string, body: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const url = `${this.apiBase}${path}`;
    const resp = await fetch(url, {
      method: "POST",
      headers: {
        "Authorization": this.authHeader(),
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
    });
    const data = await resp.json() as Record<string, unknown>;
    if (data.code && (data.code as number) !== 0) {
      throw new Error(`QQ API error [${data.code}]: ${data.message}`);
    }
    return data;
  }

  async sendText(to: string, text: string, replyTo?: string): Promise<SendReceipt> {
    const path = this.isGroupTarget(to)
      ? `/v2/groups/${to}/messages`
      : `/v2/users/${to}/messages`;
    const body: Record<string, unknown> = { content: text, msg_type: 0 };
    if (replyTo) body.msg_id = replyTo;
    const result = await this.callApi(path, body);
    return {
      success: true,
      messageId: (result.id as string) ?? "",
      timestamp: Date.now(),
    };
  }

  async sendMedia(to: string, mediaUrl: string, text?: string): Promise<SendReceipt> {
    const content = `📎 ${mediaUrl}${text ? `\n${text}` : ""}`;
    return this.sendText(to, content);
  }

  async sendPayload(to: string, payload: ReplyPayload): Promise<SendReceipt> {
    let text = payload.text || "";
    if (payload.mediaUrls?.length) {
      text += "\n" + payload.mediaUrls.map((u, i) => `📎 [${i}](${u})`).join("\n");
    }
    if (payload.isError) {
      text = `❌ ${text}`;
    }
    return this.sendText(to, text);
  }

  async healthCheck(): Promise<HealthStatus> {
    const start = Date.now();
    try {
      const resp = await fetch(`${this.apiBase}/me`, {
        headers: { "Authorization": this.authHeader() },
      });
      if (!resp.ok) {
        return { healthy: false, error: `HTTP ${resp.status}`, latencyMs: Date.now() - start };
      }
      return { healthy: true, latencyMs: Date.now() - start };
    } catch (err) {
      return { healthy: false, error: String(err), latencyMs: Date.now() - start };
    }
  }
}
// ── 通道工厂 ───────────────────────────────────────

function createChannel(config: ChannelConfig): ChannelPlugin {
  switch (config.platform) {
    case "telegram":
      return new TelegramChannel(config);
    case "feishu":
      return new FeishuChannel(config);
    case "qq":
      return new QqChannel(config);
    case "console":
      return new ConsoleChannel(config);
    default:
      console.warn(`[WARN] Unknown platform "${config.platform}", falling back to console`);
      return new ConsoleChannel({ ...config, platform: "console" });
  }
}

// ── 主程序 ─────────────────────────────────────────

async function main() {
  // 加载配置并初始化通道
  const configs = loadConfig();
  const channels = new Map<string, ChannelPlugin>();
  for (const cfg of configs) {
    const ch = createChannel(cfg);
    channels.set(ch.id, ch);
    console.error(`[INFO] 通道已加载: ${ch.id} (${ch.platform})`);
  }

  // 创建 MCP 服务器
  const server = new Server(
    { name: "openclaw-bridge", version: "1.0.0" },
    { capabilities: { tools: {} } },
  );

  // ── tools/list ──
  server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools: [
      {
        name: "channels_list",
        description: "列出所有可用消息通道",
        inputSchema: {
          type: "object",
          properties: {},
          required: [],
        },
      },
      {
        name: "channel_send_text",
        description: "通过指定通道发送纯文本消息",
        inputSchema: {
          type: "object",
          properties: {
            channel: { type: "string", description: "通道 ID (如 telegram, console)" },
            to: { type: "string", description: "目标标识 (用户ID/群ID/会话ID)" },
            text: { type: "string", description: "消息内容 (支持 HTML 格式)" },
            reply_to: { type: "string", description: "回复的消息 ID (可选)" },
          },
          required: ["channel", "to", "text"],
        },
      },
      {
        name: "channel_send_media",
        description: "通过指定通道发送媒体消息 (图片/文件)",
        inputSchema: {
          type: "object",
          properties: {
            channel: { type: "string", description: "通道 ID" },
            to: { type: "string", description: "目标标识" },
            media_url: { type: "string", description: "媒体文件 URL" },
            text: { type: "string", description: "附言文本 (可选)" },
          },
          required: ["channel", "to", "media_url"],
        },
      },
      {
        name: "channel_send_payload",
        description: "通过指定通道发送富文本/互动消息",
        inputSchema: {
          type: "object",
          properties: {
            channel: { type: "string", description: "通道 ID" },
            to: { type: "string", description: "目标标识" },
            text: { type: "string", description: "文本内容" },
            media_urls: {
              type: "array",
              items: { type: "string" },
              description: "媒体文件 URL 列表",
            },
            is_error: { type: "boolean", description: "标记为错误信息" },
          },
          required: ["channel", "to"],
        },
      },
      {
        name: "channel_health",
        description: "检查通道健康状态",
        inputSchema: {
          type: "object",
          properties: {
            channel: { type: "string", description: "通道 ID (不指定则检查所有)" },
          },
          required: [],
        },
      },
    ],
  }));

  // ── tools/call ──
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;

    try {
      switch (name) {
        case "channels_list": {
          const list = Array.from(channels.values()).map((ch) => ({
            id: ch.id,
            label: ch.label,
            platform: ch.platform,
          }));
          return {
            content: [{ type: "text" as const, text: JSON.stringify(list, null, 2) }],
          };
        }

        case "channel_send_text": {
          const { channel: chId, to, text, reply_to } = args as Record<string, string>;
          const ch = channels.get(chId);
          if (!ch) throw new Error(`Channel "${chId}" not found`);
          const receipt = await ch.sendText(to, text, reply_to);
          return {
            content: [{ type: "text" as const, text: JSON.stringify(receipt) }],
          };
        }

        case "channel_send_media": {
          const { channel: chId, to, media_url, text } = args as Record<string, string>;
          const ch = channels.get(chId);
          if (!ch) throw new Error(`Channel "${chId}" not found`);
          const receipt = await ch.sendMedia(to, media_url, text);
          return {
            content: [{ type: "text" as const, text: JSON.stringify(receipt) }],
          };
        }

        case "channel_send_payload": {
          const { channel: chId, to, text, media_urls, is_error } = args as Record<string, unknown>;
          const ch = channels.get(chId as string);
          if (!ch) throw new Error(`Channel "${chId}" not found`);
          const payload: ReplyPayload = {
            text: text as string | undefined,
            mediaUrls: media_urls as string[] | undefined,
            isError: is_error as boolean | undefined,
          };
          const receipt = await ch.sendPayload(to as string, payload);
          return {
            content: [{ type: "text" as const, text: JSON.stringify(receipt) }],
          };
        }

        case "channel_health": {
          const { channel: chId } = (args ?? {}) as { channel?: string };
          if (chId) {
            const ch = channels.get(chId);
            if (!ch) throw new Error(`Channel "${chId}" not found`);
            const status = await ch.healthCheck();
            return {
              content: [{ type: "text" as const, text: JSON.stringify({ [chId]: status }) }],
            };
          }
          // 检查所有通道
          const results: Record<string, HealthStatus> = {};
          for (const [id, ch] of channels) {
            results[id] = await ch.healthCheck();
          }
          return {
            content: [{ type: "text" as const, text: JSON.stringify(results, null, 2) }],
          };
        }

        default:
          throw new Error(`Unknown tool: ${name}`);
      }
    } catch (err) {
      return {
        content: [{ type: "text" as const, text: `Error: ${err}` }],
        isError: true,
      };
    }
  });

  // ── 启动 ──
  const transport = new StdioServerTransport();
  console.error("[INFO] openclaw-bridge MCP server starting...");
  await server.connect(transport);
  console.error("[INFO] openclaw-bridge MCP server running (stdio)");
}

main().catch((err) => {
  console.error("[FATAL]", err);
  process.exit(1);
});
