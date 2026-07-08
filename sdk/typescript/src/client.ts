/** LingShu TypeScript SDK — HTTP 客户端. */

import ky, { type KyInstance } from "ky";
import type {
  AgentRequest,
  AgentResponse,
  ChatRequest,
  ChatResponse,
  EvalRequest,
  EvalResult,
  FederationNode,
  FederationStatus,
} from "./models.js";

export class LingShuClient {
  private client: KyInstance;

  constructor(baseUrl = "http://localhost:8080", apiKey?: string) {
    const prefixUrl = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      "User-Agent": "lingshu-ts/0.1.0",
    };
    if (apiKey) headers["Authorization"] = `Bearer ${apiKey}`;
    this.client = ky.create({ prefixUrl, headers, timeout: 30_000 });
  }

  // ── Chat Completion ──

  async chat(request: ChatRequest): Promise<ChatResponse> {
    const payload: Record<string, unknown> = {
      model: request.model ?? "default",
      messages: request.messages,
    };
    if (request.temperature !== undefined) payload.temperature = request.temperature;
    if (request.maxTokens !== undefined) payload.max_tokens = request.maxTokens;
    if (request.stream) payload.stream = true;

    const data = await this.client.post("v1/chat/completions", { json: payload }).json<any>();
    return {
      id: data.id ?? "",
      content: data.choices[0].message.content,
      model: data.model ?? request.model ?? "default",
      usage: data.usage ?? {},
    };
  }

  // ── Agent Operations ──

  async runAgent(request: AgentRequest): Promise<AgentResponse> {
    const data = await this.client
      .post(`agents/${request.agentId}/run`, {
        json: { input: request.input, config: request.config ?? {} },
      })
      .json<any>();
    return {
      agentId: data.agent_id ?? request.agentId,
      status: data.status ?? "completed",
      output: data.output ?? "",
      durationMs: data.duration_ms ?? 0,
    };
  }

  async getAgentStatus(agentId: string): Promise<AgentResponse> {
    const data = await this.client.get(`agents/${agentId}/status`).json<any>();
    return {
      agentId: data.agent_id ?? agentId,
      status: data.status ?? "unknown",
      output: data.output ?? "",
      durationMs: data.duration_ms ?? 0,
    };
  }

  // ── Evaluation ──

  async runEval(request: EvalRequest): Promise<EvalResult> {
    const data = await this.client
      .post("eval/run", {
        json: {
          suite_name: request.suiteName,
          categories: request.categories ?? [],
          max_concurrency: request.maxConcurrency ?? 4,
        },
      })
      .json<any>();
    return {
      suiteName: data.suite_name ?? request.suiteName,
      total: data.total ?? 0,
      passed: data.passed ?? 0,
      failed: data.failed ?? 0,
      accuracy: data.accuracy ?? 0,
      avgLatencyMs: data.avg_latency_ms ?? 0,
      reportUrl: data.report_url,
    };
  }

  // ── Federation ──

  async getFederationStatus(): Promise<FederationStatus> {
    const data = await this.client.get("federation/status").json<any>();
    return {
      connectedNodes: data.connected_nodes ?? 0,
      totalNodes: data.total_nodes ?? 0,
      activeLinks: data.active_links ?? 0,
      uptimeSeconds: data.uptime_seconds ?? 0,
    };
  }

  async listFederationNodes(): Promise<FederationNode[]> {
    const data = await this.client.get("federation/nodes").json<any>();
    return (data.nodes ?? []).map((n: any) => ({
      nodeId: n.node_id ?? "",
      name: n.name ?? "",
      status: n.status ?? "unknown",
      capabilities: n.capabilities ?? [],
    }));
  }

  // ── Health ──

  async healthCheck(): Promise<Record<string, unknown>> {
    return this.client.get("health").json();
  }
}
