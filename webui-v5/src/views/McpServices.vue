<script setup lang="ts">
</script>

<template>
  <div class="page-padding">
    <header class="page-header">
      <button class="back-btn" @click="$router.back()">‹ 返回</button>
      <h1 class="page-title">🔌 MCP 服务</h1>
      <button class="add-btn">＋ 添加</button>
    </header>
    <div class="empty-state">
      <div class="empty-icon">🔌</div>
      <h2>MCP 服务管理</h2>
      <p>管理 Model Context Protocol 服务器与工具</p>
    </div>
  </div>
</template>

<style scoped>
.page-header {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 24px;
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
.add-btn {
  background: var(--accent-blue);
  color: #fff;
  border: none;
  border-radius: 8px;
  padding: 6px 14px;
  font-size: 0.8rem;
  font-weight: 600;
  cursor: pointer;
  font-family: inherit;
}
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
.empty-state p { font-size: 0.85rem; }
</style>
