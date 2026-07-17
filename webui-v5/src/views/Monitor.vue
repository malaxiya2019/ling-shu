<script setup lang="ts">
</script>

<template>
  <div class="page-padding">
    <header class="page-header">
      <button class="back-btn" @click="$router.back()">‹ 返回</button>
      <h1 class="page-title">📊 系统监控</h1>
    </header>

    <div class="metrics-grid">
      <div class="metric-card">
        <span class="metric-icon">💾</span>
        <span class="metric-label">内存使用</span>
        <span class="metric-value">—</span>
      </div>
      <div class="metric-card">
        <span class="metric-icon">⚡</span>
        <span class="metric-label">CPU</span>
        <span class="metric-value">—</span>
      </div>
      <div class="metric-card">
        <span class="metric-icon">📡</span>
        <span class="metric-label">请求/秒</span>
        <span class="metric-value">—</span>
      </div>
      <div class="metric-card">
        <span class="metric-icon">⏱️</span>
        <span class="metric-label">响应延迟</span>
        <span class="metric-value">—</span>
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

.metrics-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}

.metric-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 16px;
  text-align: center;
}

.metric-icon { font-size: 1.5rem; display: block; margin-bottom: 8px; }
.metric-label {
  display: block; font-size: 0.7rem;
  color: var(--text-muted); text-transform: uppercase;
  letter-spacing: 0.04em; margin-bottom: 4px;
}
.metric-value {
  display: block; font-size: 1.3rem;
  font-weight: 700; color: var(--accent-blue);
}
</style>
