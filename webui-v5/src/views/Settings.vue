<script setup lang="ts">
import { ref, computed } from 'vue'

const lang = ref<'zh-CN' | 'en'>('zh-CN')
const theme = ref<'dark' | 'light'>('dark')

const runtimeEnv = computed(() => {
  try {
    return navigator.userAgent.includes('Android') ? 'Android' : 'Web'
  } catch {
    return 'Web'
  }
})

function switchLang() {
  lang.value = lang.value === 'zh-CN' ? 'en' : 'zh-CN'
}
</script>

<template>
  <div class="page-padding">
    <header class="page-header">
      <button class="back-btn" @click="$router.back()">‹ 返回</button>
      <h1 class="page-title">⚙️ 系统设置</h1>
    </header>

    <div class="settings-group">
      <h2 class="section-title">偏好设置</h2>

      <div class="setting-item">
        <div class="setting-info">
          <span class="setting-label">界面语言</span>
          <span class="setting-desc">当前：{{ lang === 'zh-CN' ? '简体中文' : 'English' }}</span>
        </div>
        <button class="toggle-btn" @click="switchLang">
          {{ lang === 'zh-CN' ? 'English' : '简体中文' }}
        </button>
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <span class="setting-label">主题</span>
          <span class="setting-desc">当前：深色模式</span>
        </div>
        <span class="setting-badge">深色</span>
      </div>
    </div>

    <div class="settings-group">
      <h2 class="section-title">系统信息</h2>
      <div class="setting-item">
        <span class="setting-label">版本</span>
        <span class="setting-value">v5.0.0</span>
      </div>
      <div class="setting-item">
        <span class="setting-label">运行环境</span>
        <span class="setting-value">{{ runtimeEnv }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.page-header {
  display: flex; align-items: center; gap: 12px;
  margin-bottom: 20px; padding-top: 8px;
}
.back-btn {
  background: none; border: none; color: var(--accent-blue);
  font-size: 0.95rem; cursor: pointer; padding: 4px 0; font-family: inherit;
}
.page-title { font-size: 1.1rem; font-weight: 700; flex: 1; }

.settings-group {
  margin-bottom: 24px;
}

.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 14px;
  margin-bottom: 8px;
}

.setting-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.setting-label {
  font-size: 0.9rem;
  font-weight: 500;
}

.setting-desc {
  font-size: 0.75rem;
  color: var(--text-muted);
}

.setting-value {
  font-size: 0.85rem;
  color: var(--text-secondary);
}

.toggle-btn {
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 12px;
  color: var(--accent-blue);
  font-size: 0.8rem;
  cursor: pointer;
  font-family: inherit;
}

.setting-badge {
  font-size: 0.75rem;
  color: var(--text-muted);
  background: var(--bg-tertiary);
  padding: 4px 10px;
  border-radius: 12px;
}
</style>
