<script setup lang="ts">
</script>

<template>
  <div class="page-padding">
    <header class="page-header">
      <button class="back-btn" @click="$router.back()">‹ 返回</button>
      <h1 class="page-title">📁 知识库</h1>
      <div></div>
    </header>
    <div class="empty-state">
      <div class="empty-icon">📁</div>
      <h2>知识库</h2>
      <p>管理文档、记忆与知识图谱</p>
      <div class="stats-row">
        <div class="stat-card">
          <span class="stat-num">0</span>
          <span class="stat-label">文档</span>
        </div>
        <div class="stat-card">
          <span class="stat-num">0</span>
          <span class="stat-label">记忆片段</span>
        </div>
        <div class="stat-card">
          <span class="stat-num">0</span>
          <span class="stat-label">知识图谱</span>
        </div>
      </div>
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

.stats-row {
  display: flex;
  gap: 12px;
  margin-top: 24px;
}

.stat-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 16px;
  text-align: center;
  flex: 1;
}

.stat-num {
  display: block;
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--accent-blue);
}

.stat-label {
  font-size: 0.7rem;
  color: var(--text-muted);
  margin-top: 4px;
}
</style>
