<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { getHealth, getVersion, getAgents, getPlugins, formatUptime } from '@/api'
import type { HealthResponse, VersionInfo, AgentSummary, PluginItem } from '@/api/types'

const router = useRouter()

// ── State ──
const loading = ref(true)
const error = ref('')

const health = ref<HealthResponse | null>(null)
const version = ref<VersionInfo | null>(null)
const agents = ref<AgentSummary[]>([])
const plugins = ref<PluginItem[]>([])

// ── Computed status ──
const runtimeOk = ref(false)
const memoryOk = ref(false)
const mcpOk = ref(false)
const wsOk = ref(false)
const runningAgents = ref(0)
const uptime = ref('--')

const quickActions = [
  { icon: '🤖', label: '创建智能体', path: '/agents', desc: '配置一个新的 AI 智能体' },
  { icon: '💬', label: '开始对话', path: '/chat', desc: '与 AI 助手交流' },
  { icon: '📄', label: '新建工作流', path: '/workflows', desc: '编排自动化流程' },
  { icon: '📥', label: '导入插件', path: '/more', desc: '安装 MCP 或扩展插件' },
  { icon: '🔍', label: '搜索记忆', path: '/knowledge', desc: '查询知识库和记忆' },
]

// ── Fetch all data ──
async function loadData() {
  loading.value = true
  error.value = ''

  try {
    const [h, v, a, p] = await Promise.allSettled([
      getHealth(),
      getVersion(),
      getAgents(),
      getPlugins(),
    ])

    if (h.status === 'fulfilled') {
      health.value = h.value
      uptime.value = h.value.uptime
      for (const check of h.value.checks) {
        if (check.name === 'runtime') runtimeOk.value = check.healthy
        if (check.name === 'memory') memoryOk.value = check.healthy
        if (check.name === 'mcp') mcpOk.value = check.healthy
        if (check.name === 'websocket') wsOk.value = check.healthy
      }
    }

    if (v.status === 'fulfilled') {
      version.value = v.value
    }

    if (a.status === 'fulfilled') {
      agents.value = a.value.agents
      runningAgents.value = a.value.agents.filter(
        ag => ag.status === 'Running' || ag.status === 'Idle'
      ).length
    }

    if (p.status === 'fulfilled') {
      plugins.value = p.value.plugins
    }

    // Collect errors
    const errs: string[] = []
    if (h.status === 'rejected') errs.push(`health: ${h.reason}`)
    if (v.status === 'rejected') errs.push(`version: ${v.reason}`)
    if (a.status === 'rejected') errs.push(`agents: ${a.reason}`)
    if (p.status === 'rejected') errs.push(`plugins: ${p.reason}`)
    if (errs.length > 0) error.value = errs.join('; ')

  } catch (e: any) {
    error.value = e?.message || '加载失败'
  }

  loading.value = false
}

onMounted(loadData)

function goTo(path: string) {
  router.push(path)
}
</script>

<template>
  <div class="home">
    <!-- Header -->
    <header class="home-header">
      <div class="brand">
        <span class="brand-icon">⚡</span>
        <span class="brand-text">灵枢 AI 平台</span>
      </div>
      <div class="header-actions">
        <button class="icon-btn" title="刷新" @click="loadData">
          <span>{{ loading ? '⏳' : '🔄' }}</span>
        </button>
      </div>
    </header>

    <!-- Error Banner -->
    <div v-if="error" class="error-banner">
      <span>⚠️ {{ error }}</span>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="loading-state">
      <div class="spinner"></div>
      <span>加载系统状态…</span>
    </div>

    <template v-else>
      <!-- System Status -->
      <section class="status-banner">
        <div class="status-row">
          <span
            class="status-dot"
            :style="{ background: runtimeOk ? 'var(--accent-green)' : 'var(--accent-red)' }"
          ></span>
          <span class="status-label">系统状态</span>
          <span
            class="status-value"
            :style="{ color: runtimeOk ? 'var(--accent-green)' : 'var(--accent-red)' }"
          >
            {{ runtimeOk ? '🟢 运行正常' : '🔴 异常' }}
          </span>
        </div>
        <div class="uptime-row">
          <span class="uptime-label">已运行</span>
          <span class="uptime-value">{{ uptime }}</span>
          <span v-if="version" class="version-badge">{{ version.version }}</span>
        </div>
      </section>

      <!-- Overview Grid -->
      <section class="overview-grid">
        <!-- AI Model -->
        <div class="overview-card" @click="goTo('/chat')">
          <div class="card-icon">🧠</div>
          <div class="card-info">
            <span class="card-label">AI 模型</span>
            <span class="card-value">{{ health?.checks.find(c => c.name === 'runtime')?.detail || '—' }}</span>
          </div>
        </div>

        <!-- Agents -->
        <div class="overview-card" @click="goTo('/agents')">
          <div class="card-icon">🤖</div>
          <div class="card-info">
            <span class="card-label">智能体</span>
            <span class="card-value">{{ agents.length }} 个</span>
            <span class="card-sub">运行中 {{ runningAgents }}</span>
          </div>
        </div>

        <!-- Memory -->
        <div class="overview-card" @click="goTo('/knowledge')">
          <div class="card-icon">💾</div>
          <div class="card-info">
            <span class="card-label">记忆中心</span>
            <span class="card-value" :style="{ color: memoryOk ? 'var(--accent-green)' : 'var(--accent-yellow)' }">
              {{ memoryOk ? '✅ 已加载' : '⚠️ 不可用' }}
            </span>
          </div>
        </div>

        <!-- MCP -->
        <div class="overview-card" @click="goTo('/mcp')">
          <div class="card-icon">🔌</div>
          <div class="card-info">
            <span class="card-label">MCP 服务</span>
            <span class="card-value" :style="{ color: mcpOk ? 'var(--accent-green)' : 'var(--text-muted)' }">
              {{ mcpOk ? '✅ 在线' : '⚪ 未配置' }}
            </span>
          </div>
        </div>

        <!-- Federation -->
        <div class="overview-card" @click="goTo('/federation')">
          <div class="card-icon">🌐</div>
          <div class="card-info">
            <span class="card-label">联邦网络</span>
            <span class="card-value">—</span>
          </div>
        </div>

        <!-- Workflows -->
        <div class="overview-card" @click="goTo('/workflows')">
          <div class="card-icon">🔄</div>
          <div class="card-info">
            <span class="card-label">WebSocket</span>
            <span class="card-value" :style="{ color: wsOk ? 'var(--accent-green)' : 'var(--text-muted)' }">
              {{ wsOk ? '✅ 已连接' : '⚪ 未连接' }}
            </span>
          </div>
        </div>

        <!-- Plugins (span 2 cols) -->
        <div class="overview-card plugins-card" @click="goTo('/more')">
          <div class="card-icon">🧩</div>
          <div class="card-info">
            <span class="card-label">已加载插件</span>
            <span class="card-value">{{ plugins.length }} 个</span>
            <div class="plugin-chips" v-if="plugins.length > 0">
              <span v-for="p in plugins.slice(0, 5)" :key="p.id" class="plugin-chip">
                {{ p.name }}
              </span>
              <span v-if="plugins.length > 5" class="plugin-chip more">+{{ plugins.length - 5 }}</span>
            </div>
          </div>
        </div>
      </section>

      <!-- Divider -->
      <div class="section-divider"></div>

      <!-- Quick Actions -->
      <section class="quick-actions">
        <h2 class="section-title">快捷操作</h2>
        <div class="actions-list">
          <button
            v-for="action in quickActions"
            :key="action.label"
            class="action-btn"
            @click="goTo(action.path)"
          >
            <span class="action-icon">{{ action.icon }}</span>
            <div class="action-text">
              <span class="action-label">{{ action.label }}</span>
              <span class="action-desc">{{ action.desc }}</span>
            </div>
            <span class="action-arrow">›</span>
          </button>
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped>
.home {
  padding: 16px 16px 8px;
  max-width: 600px;
  margin: 0 auto;
}

.home-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 20px;
  padding-top: 8px;
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
}

.brand-icon { font-size: 1.5rem; }

.brand-text {
  font-size: 1.1rem;
  font-weight: 700;
  background: linear-gradient(135deg, var(--accent-blue), var(--accent-purple));
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}

.icon-btn {
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 50%;
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 1rem;
  transition: border-color 0.15s;
}

.icon-btn:hover { border-color: var(--accent-blue); }

/* Loading */
.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 80px 0;
  color: var(--text-muted);
  font-size: 0.9rem;
}

.spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--border);
  border-top-color: var(--accent-blue);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }

/* Error */
.error-banner {
  background: rgba(248, 81, 73, 0.1);
  border: 1px solid rgba(248, 81, 73, 0.3);
  border-radius: var(--radius);
  padding: 10px 14px;
  margin-bottom: 16px;
  font-size: 0.8rem;
  color: var(--accent-red);
}

/* Status Banner */
.status-banner {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 16px;
  margin-bottom: 16px;
}

.status-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.status-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  box-shadow: 0 0 8px currentColor;
  animation: pulse-dot 2s ease-in-out infinite;
}

@keyframes pulse-dot {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.status-label {
  font-size: 0.85rem;
  color: var(--text-secondary);
}

.status-value {
  margin-left: auto;
  font-weight: 600;
  font-size: 0.9rem;
}

.uptime-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
  padding-left: 20px;
}

.uptime-label {
  font-size: 0.75rem;
  color: var(--text-muted);
}

.uptime-value {
  font-size: 0.8rem;
  font-weight: 500;
  color: var(--text-secondary);
}

.version-badge {
  margin-left: auto;
  font-size: 0.65rem;
  color: var(--accent-cyan);
  background: rgba(88, 166, 255, 0.1);
  border: 1px solid rgba(88, 166, 255, 0.2);
  padding: 2px 8px;
  border-radius: 10px;
}

/* Overview Grid */
.overview-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin-bottom: 20px;
}

.overview-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 14px;
  cursor: pointer;
  transition: border-color 0.15s, transform 0.1s;
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.overview-card:active { transform: scale(0.97); }
.overview-card:hover { border-color: var(--accent-blue); }

.plugins-card { grid-column: span 2; }

.card-icon { font-size: 1.4rem; flex-shrink: 0; margin-top: 2px; }

.card-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.card-label {
  font-size: 0.7rem;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.card-value {
  font-size: 0.9rem;
  font-weight: 600;
  color: var(--text-primary);
}

.card-sub {
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.plugin-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 4px;
}

.plugin-chip {
  font-size: 0.65rem;
  padding: 2px 6px;
  background: var(--bg-tertiary);
  border-radius: 4px;
  color: var(--text-muted);
  white-space: nowrap;
}

.plugin-chip.more {
  color: var(--accent-blue);
}

/* Section */
.section-divider {
  height: 1px;
  background: var(--border);
  margin: 4px 0 20px;
}

.section-title {
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 12px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

/* Quick Actions */
.actions-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.action-btn {
  display: flex;
  align-items: center;
  gap: 12px;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 14px;
  cursor: pointer;
  transition: border-color 0.15s, transform 0.1s;
  text-align: left;
  color: inherit;
  width: 100%;
  font-family: inherit;
  font-size: inherit;
}

.action-btn:active { transform: scale(0.98); }
.action-btn:hover { border-color: var(--accent-blue); }

.action-icon { font-size: 1.3rem; flex-shrink: 0; }

.action-text {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.action-label {
  font-size: 0.9rem;
  font-weight: 600;
  color: var(--text-primary);
}

.action-desc {
  font-size: 0.75rem;
  color: var(--text-muted);
}

.action-arrow {
  font-size: 1.2rem;
  color: var(--text-muted);
  flex-shrink: 0;
}
</style>
