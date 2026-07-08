/** LingShu SDK — 数据模型. */

export interface ChatMessage {
  role: "user" | "assistant" | "system";
  content: string;
}

export interface ChatRequest {
  model?: string;
  messages: ChatMessage[];
  temperature?: number;
  maxTokens?: number;
  stream?: boolean;
}

export interface ChatResponse {
  id: string;
  content: string;
  model: string;
  usage: Record<string, number>;
}

export interface AgentRequest {
  agentId: string;
  input: string;
  config?: Record<string, unknown>;
}

export interface AgentResponse {
  agentId: string;
  status: string;
  output: string;
  durationMs: number;
}

export interface EvalRequest {
  suiteName: string;
  categories?: string[];
  maxConcurrency?: number;
}

export interface EvalResult {
  suiteName: string;
  total: number;
  passed: number;
  failed: number;
  accuracy: number;
  avgLatencyMs: number;
  reportUrl?: string;
}

export interface FederationNode {
  nodeId: string;
  name: string;
  status: string;
  capabilities: string[];
}

export interface FederationStatus {
  connectedNodes: number;
  totalNodes: number;
  activeLinks: number;
  uptimeSeconds: number;
}
