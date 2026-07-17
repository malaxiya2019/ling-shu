// ════════════════════════════════════════════════════════════
// 灵枢 v5 — API 客户端
// 所有与后端通信的接口集中在此
// ════════════════════════════════════════════════════════════

import type {
  HealthResponse,
  VersionInfo,
  AgentListResponse,
  ChatRequest,
  ChatResponse,
  FederationStatus,
  FederationNodeInfo,
  PluginListResponse,
  ModelInfo,
  RuntimeStatusResponse,
} from './types'

const BASE = ''  // 通过 Vite proxy 代理到后端

// ── 通用请求 ──
async function getJSON<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    credentials: 'include',
  })
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}: ${res.statusText}`)
  }
  return res.json()
}

async function postJSON<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify(body),
  })
  if (!res.ok) {
    const err = await res.text().catch(() => '')
    throw new Error(`HTTP ${res.status}: ${err || res.statusText}`)
  }
  return res.json()
}

// ── System ──
export async function getHealth(): Promise<HealthResponse> {
  return getJSON<HealthResponse>('/health')
}

export async function getVersion(): Promise<VersionInfo> {
  return getJSON<VersionInfo>('/version')
}

export async function getModels(): Promise<ModelInfo[]> {
  return getJSON<ModelInfo[]>('/v1/models')
}

// ── Runtime ──
export async function getRuntimeStatus(): Promise<RuntimeStatusResponse> {
  return getJSON<RuntimeStatusResponse>('/v1/agents')
    .then(res => ({
      state: 'Running',
      agent_count: res.agents.length,
      session_count: 0,
    }))
}

// ── Agents ──
export async function getAgents(): Promise<AgentListResponse> {
  return getJSON<AgentListResponse>('/v1/agents')
}

// ── Chat ──
export async function sendChat(req: ChatRequest): Promise<ChatResponse> {
  return postJSON<ChatResponse>('/v1/chat', req)
}

// ── Federation ──
export async function getFederationStatus(): Promise<FederationStatus> {
  return getJSON<FederationStatus>('/v1/federation/status')
}

export async function getFederationNodes(): Promise<FederationNodeInfo[]> {
  return getJSON<FederationNodeInfo[]>('/v1/federation/nodes')
}

// ── Plugins ──
export async function getPlugins(): Promise<PluginListResponse> {
  return getJSON<PluginListResponse>('/v1/plugins')
}

// ── Misc ──
export function formatUptime(seconds: number): string {
  const d = Math.floor(seconds / 86400)
  const h = Math.floor((seconds % 86400) / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const parts: string[] = []
  if (d > 0) parts.push(`${d}d`)
  if (h > 0) parts.push(`${h}h`)
  if (m > 0) parts.push(`${m}m`)
  return parts.join(' ') || '<1m'
}
