<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'

const router = useRouter()
const showDevTools = ref(false)

const mainSections = [
  { name: 'knowledge', label: '知识库', icon: '📁', desc: '文档、记忆、知识管理', path: '/knowledge' },
  { name: 'mcp', label: 'MCP 服务', icon: '🔌', desc: '管理 MCP 服务器与工具', path: '/mcp' },
  { name: 'federation', label: '联邦网络', icon: '🌐', desc: '跨节点协作与同步', path: '/federation' },
  { name: 'monitor', label: '系统监控', icon: '📊', desc: '性能指标与日志', path: '/monitor' },
  { name: 'settings', label: '系统设置', icon: '⚙️', desc: '平台配置与偏好', path: '/settings' },
]

const devTools = [
  { name: 'api-docs', label: '接口文档', icon: '📄', path: '/dev/api-docs' },
  { name: 'logs', label: '系统日志', icon: '📝', path: '/dev/logs' },
  { name: 'hot-reload', label: '热重载', icon: '🔥', path: '/dev/hot-reload' },
  { name: 'debug', label: '调试工具', icon: '🧪', path: '/dev/debug' },
]

function goTo(path: string) {
  router.push(path)
}
</script>

<template>
  <div class="page-padding">
    <header class="more-header">
      <h1 class="page-title">📋 更多功能</h1>
    </header>

    <!-- Main secondary functions -->
    <section class="section">
      <h2 class="section-title">功能</h2>
      <div class="apps-grid">
        <button
          v-for="app in mainSections"
          :key="app.name"
          class="app-item"
          @click="goTo(app.path)"
        >
          <span class="app-icon">{{ app.icon }}</span>
          <span class="app-label">{{ app.label }}</span>
          <span class="app-desc">{{ app.desc }}</span>
        </button>
      </div>
    </section>

    <!-- Developer Tools (collapsed by default) -->
    <section class="section">
      <button class="dev-toggle" @click="showDevTools = !showDevTools">
        <span class="dev-toggle-icon">🛠️</span>
        <span>开发者工具</span>
        <span class="chevron" :class="{ open: showDevTools }">▾</span>
      </button>

      <div v-if="showDevTools" class="dev-list">
        <button
          v-for="tool in devTools"
          :key="tool.name"
          class="dev-item"
          @click="goTo(tool.path)"
        >
          <span class="dev-icon">{{ tool.icon }}</span>
          <span class="dev-label">{{ tool.label }}</span>
        </button>
      </div>
    </section>

    <!-- Version info -->
    <div class="version-info">
      <span>灵枢 AI 平台 v5.0.0</span>
      <span class="build-info">Build 20260717</span>
    </div>
  </div>
</template>

<style scoped>
.more-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 20px;
  padding-top: 8px;
}

.page-title { font-size: 1.1rem; font-weight: 700; }

/* ── Main Apps Grid ── */
.apps-grid {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.app-item {
  display: flex;
  align-items: center;
  gap: 12px;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 14px;
  cursor: pointer;
  transition: border-color 0.15s, transform 0.1s;
  text-align: left;
  color: inherit;
  width: 100%;
  font-family: inherit;
  font-size: inherit;
}

.app-item:active { transform: scale(0.98); }
.app-item:hover { border-color: var(--accent-blue); }

.app-icon { font-size: 1.4rem; flex-shrink: 0; }

.app-info { flex: 1; }

.app-label {
  display: block;
  font-weight: 600;
  font-size: 0.9rem;
  margin-bottom: 2px;
}

.app-desc {
  display: block;
  font-size: 0.75rem;
  color: var(--text-muted);
}

/* ── Developer Tools ── */
.dev-toggle {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 14px;
  cursor: pointer;
  color: var(--text-secondary);
  font-size: 0.9rem;
  font-weight: 600;
  transition: border-color 0.15s;
  font-family: inherit;
}

.dev-toggle:hover { border-color: var(--accent-blue); }

.dev-toggle-icon { font-size: 1.1rem; }

.chevron {
  margin-left: auto;
  transition: transform 0.2s;
  font-size: 0.8rem;
}

.chevron.open { transform: rotate(180deg); }

.dev-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
  padding-left: 8px;
}

.dev-item {
  display: flex;
  align-items: center;
  gap: 12px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--radius);
  padding: 10px 12px;
  cursor: pointer;
  transition: background 0.15s;
  text-align: left;
  color: var(--text-secondary);
  width: 100%;
  font-family: inherit;
  font-size: inherit;
}

.dev-item:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.dev-icon { font-size: 1rem; }
.dev-label { font-size: 0.85rem; }

/* ── Version ── */
.version-info {
  margin-top: 32px;
  padding: 16px 0;
  text-align: center;
  font-size: 0.75rem;
  color: var(--text-muted);
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.build-info {
  font-size: 0.7rem;
  color: var(--text-muted);
  opacity: 0.6;
}
</style>
