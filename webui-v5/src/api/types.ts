// ════════════════════════════════════════════════════════════
// 灵枢 v5 — API 类型定义
// 对应后端 Rust 数据结构
// ════════════════════════════════════════════════════════════

// ── Health / System ──
export interface HealthResponse {
  status: string
  version: string
  uptime: string
  checks: HealthCheckItem[]
}

export interface HealthCheckItem {
  name: string
  healthy: boolean
  detail?: string
}

export interface VersionInfo {
  version: string
  build_date?: string
  commit?: string
}

// ── Models ──
export interface ModelInfo {
  id: string
  name: string
  provider: string
  capabilities: string[]
}

// ── Agents ──
export interface AgentSummary {
  agent_id: string
  name: string
  status: string
  created_at?: string
}

export interface AgentListResponse {
  agents: AgentSummary[]
}

// ── Chat ──
export interface ChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
}

export interface ChatRequest {
  model?: string
  messages: ChatMessage[]
  stream?: boolean
  session_id?: string
}

export interface ChatResponse {
  message: ChatMessage
  session_id?: string
}

// ── Federation ──
export interface FederationStatus {
  cluster_id: string
  cluster_name: string
  enabled: boolean
  node_count: number
  uptime_secs: number
}

export interface FederationNodeInfo {
  id: string
  name: string
  addr: string
  status: string
  capabilities: string[]
  last_seen: string
  cluster_name?: string
}

// ── Plugins ──
export interface PluginItem {
  id: string
  name: string
  version: string
  description?: string
  status: string
  author?: string
}

export interface PluginListResponse {
  plugins: PluginItem[]
  total: number
}

// ── MCP ──
export interface McpToolInfo {
  name: string
  description: string
  server: string
}

// ── Metrics ──
export interface MetricsSnapshot {
  timestamp: string
  cpu_percent: number
  memory_mb: number
  requests_per_sec: number
  avg_latency_ms: number
}

// ── Runtime Status ──
export interface RuntimeStatusResponse {
  state: string
  agent_count: number
  session_count: number
}
