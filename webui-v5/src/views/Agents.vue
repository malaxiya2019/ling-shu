<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { getAgents } from '@/api'
import type { AgentSummary } from '@/api/types'

const agents = ref<AgentSummary[]>([])
const loading = ref(true)
const error = ref('')

const runningCount = computed(() =>
  agents.value.filter(a => a.status === 'Running' || a.status === 'Idle').length
)

onMounted(async () => {
  try {
    agents.value = (await getAgents()).agents
  } catch (e: any) {
    error.value = e?.message || '加载失败'
  }
  loading.value = false
})

function statusClass(s: string): string {
  if (s === 'Running' || s === 'Idle') return 'online'
  if (s === 'Paused') return 'idle'
  return 'offline'
}

function statusLabel(s: string): string {
  if (s === 'Running') return '运行中'
  if (s === 'Idle') return '空闲'
  if (s === 'Paused') return '已暂停'
  if (s === 'Stopped') return '已停止'
  return s
}
</script>

<template>
  <div class="page-padding">
    <header class="page-header">
      <h1 class="page-title">🤖 智能体</h1>
      <div class="header-right">
        <span class="count-badge">{{ agents.length }} 个</span>
        <button class="create-btn">＋ 创建</button>
      </div>
    </header>

    <div v-if="loading" class="loading-state">
      <div class="spinner"></div>
      <span>加载智能体列表…</span>
    </div>

    <div v-else-if="error" class="error-banner">
      ⚠️ {{ error }}
    </div>

    <template v-else>
      <div v-if="agents.length > 0" class="stats-row">
        <div class="stat">
          <span class="stat-num">{{ agents.length }}</span>
          <span class="stat-label">总计</span>
        </div>
        <div class="stat">
          <span class="stat-num" style="color: var(--accent-green)">{{ runningCount }}</span>
          <span class="stat-label">运行中</span>
        </div>
        <div class="stat">
          <span class="stat-num" style="color: var(--text-muted)">{{ agents.length - runningCount }}</span>
          <span class="stat-label">其他</span>
        </div>
      </div>

      <div v-if="agents.length === 0" class="empty-state">
        <div class="empty-icon">🤖</div>
        <h2>暂无智能体</h2>
        <p>创建一个新的 AI 智能体来开始使用</p>
        <button class="create-btn primary-btn">＋ 创建第一个智能体</button>
      </div>

      <div v-else class="agent-list">
        <div
          v-for="agent in agents"
          :key="agent.agent_id"
          class="agent-card"
        >
          <div class="agent-avatar">
            <span class="status-indicator" :class="statusClass(agent.status)"></span>
            <span class="avatar-icon">🤖</span>
          </div>
          <div class="agent-info">
            <span class="agent-name">{{ agent.name }}</span>
            <span class="agent-id">{{ agent.agent_id.slice(0, 12) }}…</span>
            <span v-if="agent.created_at" class="agent-created">{{ agent.created_at }}</span>
          </div>
          <span class="agent-status" :class="statusClass(agent.status)">{{ statusLabel(agent.status) }}</span>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
  padding-top: 8px;
}

.page-title { font-size: 1.1rem; font-weight: 700; }

.header-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.count-badge {
  font-size: 0.75rem;
  color: var(--text-muted);
}

.create-btn {
  background: var(--accent-blue);
  color: #fff;
  border: none;
  border-radius: 8px;
  padding: 8px 16px;
  font-size: 0.85rem;
  font-weight: 600;
  cursor: pointer;
  font-family: inherit;
  transition: opacity 0.15s;
}

.create-btn:active { opacity: 0.8; }

.primary-btn {
  margin-top: 12px;
  padding: 10px 20px;
}

.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 60px 0;
  gap: 12px;
  color: var(--text-muted);
}

.spinner {
  width: 28px;
  height: 28px;
  border: 3px solid var(--border);
  border-top-color: var(--accent-blue);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }

.error-banner {
  background: rgba(248,81,73,0.1);
  border: 1px solid rgba(248,81,73,0.3);
  border-radius: var(--radius);
  padding: 10px 14px;
  font-size: 0.85rem;
  color: var(--accent-red);
}

.stats-row {
  display: flex;
  gap: 10px;
  margin-bottom: 16px;
}

.stat {
  flex: 1;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px;
  text-align: center;
}

.stat-num {
  display: block;
  font-size: 1.4rem;
  font-weight: 700;
}

.stat-label {
  font-size: 0.7rem;
  color: var(--text-muted);
  margin-top: 2px;
}

.agent-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.agent-card {
  display: flex;
  align-items: center;
  gap: 12px;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 14px;
  cursor: pointer;
  transition: border-color 0.15s;
}

.agent-card:active { transform: scale(0.98); }
.agent-card:hover { border-color: var(--accent-blue); }

.agent-avatar {
  position: relative;
  font-size: 1.5rem;
}

.status-indicator {
  position: absolute;
  bottom: 0;
  right: -2px;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  border: 2px solid var(--bg-secondary);
}

.status-indicator.online { background: var(--accent-green); }
.status-indicator.idle { background: var(--accent-yellow); }
.status-indicator.offline { background: var(--text-muted); }

.agent-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.agent-name {
  font-weight: 600;
  font-size: 0.9rem;
}

.agent-id {
  font-size: 0.7rem;
  color: var(--text-muted);
  font-family: monospace;
}

.agent-created {
  font-size: 0.7rem;
  color: var(--text-muted);
}

.agent-status {
  font-size: 0.75rem;
  padding: 3px 8px;
  border-radius: 12px;
  font-weight: 500;
  flex-shrink: 0;
}

.agent-status.online { color: var(--accent-green); background: rgba(63,185,80,0.1); }
.agent-status.idle { color: var(--accent-yellow); background: rgba(210,153,34,0.1); }
.agent-status.offline { color: var(--text-muted); background: rgba(110,122,138,0.1); }

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 60px 0;
  gap: 8px;
  color: var(--text-muted);
}

.empty-icon { font-size: 3rem; }
.empty-state h2 { font-size: 1.1rem; color: var(--text-secondary); }
.empty-state p { font-size: 0.85rem; text-align: center; }
</style>
