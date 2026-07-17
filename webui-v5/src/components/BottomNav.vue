<script setup lang="ts">
import { computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'

const router = useRouter()
const route = useRoute()

// Route names that should highlight each tab
const tabRoutes: Record<string, string[]> = {
  home: ['home'],
  chat: ['chat'],
  agents: ['agents'],
  workflows: ['workflows'],
  more: ['more', 'knowledge', 'mcp', 'federation', 'monitor', 'settings',
         'api-docs', 'logs', 'hot-reload', 'debug'],
}

const navItems = [
  { name: 'home', label: '首页', icon: '🏠', path: '/' },
  { name: 'chat', label: '对话', icon: '💬', path: '/chat' },
  { name: 'agents', label: '智能体', icon: '🤖', path: '/agents' },
  { name: 'workflows', label: '工作流', icon: '🔄', path: '/workflows' },
  { name: 'more', label: '更多', icon: '📋', path: '/more' },
]

const activeTab = computed(() => {
  const name = route.name as string
  for (const [tab, routes] of Object.entries(tabRoutes)) {
    if (routes.includes(name)) return tab
  }
  return name
})

function navigate(path: string) {
  router.push(path)
}
</script>

<template>
  <nav class="bottom-nav">
    <button
      v-for="item in navItems"
      :key="item.name"
      :class="['nav-btn', { active: activeTab === item.name }]"
      @click="navigate(item.path)"
    >
      <span class="nav-icon">{{ item.icon }}</span>
      <span class="nav-label">{{ item.label }}</span>
    </button>
  </nav>
</template>

<style scoped>
.bottom-nav {
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  height: var(--nav-height);
  background: var(--bg-nav);
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-around;
  padding: 0 env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);
  z-index: 100;
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}

.nav-btn {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 4px 12px;
  min-width: 56px;
  transition: color 0.15s, transform 0.1s;
  -webkit-tap-highlight-color: transparent;
  position: relative;
}

.nav-btn:active {
  transform: scale(0.92);
}

.nav-btn.active {
  color: var(--accent-blue);
}

.nav-btn.active::before {
  content: '';
  position: absolute;
  top: 0;
  left: 50%;
  transform: translateX(-50%);
  width: 20px;
  height: 3px;
  background: var(--accent-blue);
  border-radius: 0 0 3px 3px;
}

.nav-icon {
  font-size: 1.3rem;
  line-height: 1;
}

.nav-label {
  font-size: 0.65rem;
  font-weight: 500;
  letter-spacing: 0.01em;
}
</style>
