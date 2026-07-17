<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { getFederationStatus, getFederationNodes, formatUptime } from '@/api'
import type { FederationStatus, FederationNodeInfo } from '@/api/types'

const status = ref<FederationStatus | null>(null)
const nodes = ref<FederationNodeInfo[]>([])
const loading = ref(true)
const error = ref('')

const countdown = ref('--')
let timer: number | null = null

onMounted(async () => {
  await loadData()
  // Countdown timer for next scan display
  let sec = 30
  timer = window.setInterval(() => {
    sec--
    if (sec <= 0) {
      sec = 30
      loadData()
    }
    countdown.value = `${sec}s`
  }, 1000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})

async function loadData() {
  try {
    const [s, n] = await Promise.all([
      getFederationStatus().catch(() => null),
      getFederationNodes().catch(() => []),
    ])
    if (s) status.value = s
    nodes.value = n
  } catch (e: any) {
    error.value = e?.message || '加载失败'
  }
  loading.value = false
}

function statusClass(s: string): string {
  if (s === 'online') return 'online'
  if (s === 'offline') return 'offline'
  return 'degraded'
}
</script>

<template>
  <div class="page-padding">
    <header class="page-header">
      <button class="back-btn" @click="$router.back()">‹ 返回</button>
      <h1 class="page-title">🌐 联邦网络</h1>
      <button class="refresh-btn" @click="loadData">{{ loading ? '⏳' : '↻' }}</button>
    </header>

    <div v-if="loading" class="loading-state">
      <div class="spinner"></div>
      <span>加载联邦状态…</span>
    </div>

    <template v-else>
      <!-- Status Card -->
      <div class="fed-status-card">
        <div class="fed-status-header">
          <span class="fed-dot" :class="{ active: status?.enabled }"></span>
          <span class="fed-status-text" v-if="status?.enabled">联邦网络运行中</span>
          <span class="fed-status-text" v-else>已禁用</span>
        </div>

        <div class="fed-stats">
          <div class="fed-stat">
            <span class="fed-stat-num">{{ nodes.filter(n => n.status === 'online').length }}</span>
            <span class="fed-stat-label">在线节点</span>
          </div>
          <div class="fed-stat">
            <span class="fed-stat-num">{{ nodes.filter(n => n.status === 'offline').length }}</span>
            <span class="fed-stat-label">离线节点</span>
          </div>
          <div class="fed-stat">
            <span class="fed-stat-num">{{ status?.node_count ?? nodes.length }}</span>
            <span class="fed-stat-label">总计</span>
          </div>
        </div>

        <div class="fed-timeline">
          <div class="fed-tl-item">
            <span class="fed-tl-label">集群</span>
            <span class="fed-tl-value">{{ status?.cluster_name || '—' }}</span>
          </div>
          <div class="fed-tl-item">
            <span class="fed-tl-label">下次扫描</span>
            <span class="fed-tl-value">{{ countdown }}</span>
          </div>
        </div>
      </div>

      <!-- Error -->
      <div v-if="error" class="error-banner">⚠️ {{ error }}</div>

      <!-- Node List -->
      <div v-if="nodes.length > 0" class="section">
        <h2 class="section-title">对等节点</h2>
        <div class="node-list">
          <div v-for="node in nodes" :key="node.id" class="node-card">
            <div class="node-icon">
              <span class="node-dot" :class="statusClass(node.status)"></span>
              <span>🖥️</span>
            </div>
            <div class="node-info">
              <span class="node-name">{{ node.name }}</span>
              <span class="node-addr"><code>{{ node.addr }}</code></span>
              <span v-if="node.capabilities.length > 0" class="node-caps">
                {{ node.capabilities.join(' · ') }}
              </span>
            </div>
            <span class="node-status" :class="statusClass(node.status)">
              {{ node.status === 'online' ? '在线' : node.status === 'offline' ? '离线' : node.status }}
            </span>
          </div>
        </div>
      </div>

      <!-- Empty state -->
      <div v-else-if="!loading" class="empty-state">
        <div class="empty-icon">🌐</div>
        <h2>暂无节点</h2>
        <p>尚未发现其他联邦节点。确保其他实例已启用 Federation 并在同一网络下。</p>
      </div>
    </template>
  </div>
</template>

<style scoped>
.page-header {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 20px;
  padding-top: 8px;
}

.back-btn {
  background: none;
  border: none;
  color: var(--accent-blue);
  font-size: 0.95rem;
  cursor: pointer;
  padding: 4px 0;
  font-family: inherit;
}

.page-title { font-size: 1.1rem; font-weight: 700; flex: 1; }

.refresh-btn {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 1rem;
  display: flex;
  align-items: center;
  justify-content: center;
  font-family: inherit;
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
  width: 28px; height: 28px;
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
  margin-bottom: 16px;
  font-size: 0.85rem;
  color: var(--accent-red);
}

.fed-status-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 16px;
  margin-bottom: 16px;
}

.fed-status-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 16px;
}

.fed-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--text-muted);
}

.fed-dot.active {
  background: var(--accent-green);
  box-shadow: 0 0 8px var(--accent-green);
  animation: pulse-dot 2s ease-in-out infinite;
}

@keyframes pulse-dot {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.fed-status-text {
  font-size: 0.9rem;
  font-weight: 600;
}

.fed-stats {
  display: flex;
  gap: 10px;
  margin-bottom: 16px;
}

.fed-stat {
  flex: 1;
  text-align: center;
  background: var(--bg-tertiary);
  border-radius: var(--radius);
  padding: 12px 8px;
}

.fed-stat-num {
  display: block;
  font-size: 1.4rem;
  font-weight: 700;
  color: var(--accent-blue);
}

.fed-stat-label {
  font-size: 0.7rem;
  color: var(--text-muted);
  margin-top: 2px;
}

.fed-timeline {
  display: flex;
  justify-content: space-between;
  padding-top: 12px;
  border-top: 1px solid var(--border);
}

.fed-tl-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.fed-tl-label {
  font-size: 0.7rem;
  color: var(--text-muted);
}

.fed-tl-value {
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--text-primary);
}

.node-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.node-card {
  display: flex;
  align-items: center;
  gap: 12px;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 14px;
}

.node-icon {
  position: relative;
  font-size: 1.3rem;
}

.node-dot {
  position: absolute;
  top: -2px;
  right: -4px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  border: 2px solid var(--bg-secondary);
}
.node-dot.online { background: var(--accent-green); }
.node-dot.offline { background: var(--accent-red); }
.node-dot.degraded { background: var(--accent-yellow); }

.node-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.node-name {
  font-weight: 600;
  font-size: 0.9rem;
}

.node-addr code {
  font-size: 0.75rem;
  color: var(--text-muted);
}

.node-caps {
  font-size: 0.7rem;
  color: var(--accent-cyan);
}

.node-status {
  font-size: 0.7rem;
  padding: 3px 8px;
  border-radius: 12px;
  font-weight: 500;
  flex-shrink: 0;
}

.node-status.online { color: var(--accent-green); background: rgba(63,185,80,0.1); }
.node-status.offline { color: var(--accent-red); background: rgba(248,81,73,0.1); }
.node-status.degraded { color: var(--accent-yellow); background: rgba(210,153,34,0.1); }

.section { margin-bottom: 16px; }

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 40px 0;
  gap: 8px;
  color: var(--text-muted);
}
.empty-icon { font-size: 3rem; }
.empty-state h2 { font-size: 1.1rem; color: var(--text-secondary); }
.empty-state p { font-size: 0.8rem; text-align: center; max-width: 280px; line-height: 1.6; }
</style>
