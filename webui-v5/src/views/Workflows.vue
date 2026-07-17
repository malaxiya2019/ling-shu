<script setup lang="ts">
const workflows = [
  { name: '每日报告生成', status: '运行中', lastRun: '2 分钟前', statusClass: 'online' },
  { name: '代码审查流水线', status: '运行中', lastRun: '15 分钟前', statusClass: 'online' },
  { name: '客服自动回复', status: '已暂停', lastRun: '1 小时前', statusClass: 'idle' },
]
</script>

<template>
  <div class="page-padding">
    <header class="page-header">
      <h1 class="page-title">🔄 工作流</h1>
      <button class="create-btn">＋ 新建</button>
    </header>

    <div class="workflow-list">
      <div v-for="wf in workflows" :key="wf.name" class="wf-card">
        <div class="wf-icon">
          <span class="wf-dot" :class="wf.statusClass"></span>
          <span>🔄</span>
        </div>
        <div class="wf-info">
          <span class="wf-name">{{ wf.name }}</span>
          <span class="wf-meta">最后运行：{{ wf.lastRun }}</span>
        </div>
        <span class="wf-status" :class="wf.statusClass">{{ wf.status }}</span>
      </div>
    </div>

    <div class="quick-start">
      <h2 class="section-title">快速开始</h2>
      <div class="template-list">
        <div class="template-card">
          <span class="tpl-icon">📊</span>
          <span class="tpl-label">数据采集</span>
        </div>
        <div class="template-card">
          <span class="tpl-icon">🤖</span>
          <span class="tpl-label">AI 对话</span>
        </div>
        <div class="template-card">
          <span class="tpl-icon">📧</span>
          <span class="tpl-label">邮件处理</span>
        </div>
        <div class="template-card">
          <span class="tpl-icon">🔗</span>
          <span class="tpl-label">Webhook</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 20px;
  padding-top: 8px;
}

.page-title { font-size: 1.1rem; font-weight: 700; }

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
}

.workflow-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-bottom: 24px;
}

.wf-card {
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

.wf-card:hover { border-color: var(--accent-blue); }
.wf-card:active { transform: scale(0.98); }

.wf-icon {
  position: relative;
  font-size: 1.3rem;
}

.wf-dot {
  position: absolute;
  top: -2px;
  right: -2px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  border: 2px solid var(--bg-secondary);
}

.wf-dot.online { background: var(--accent-green); }
.wf-dot.idle { background: var(--accent-yellow); }
.wf-dot.offline { background: var(--text-muted); }

.wf-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.wf-name { font-weight: 600; font-size: 0.9rem; }
.wf-meta { font-size: 0.75rem; color: var(--text-muted); }

.wf-status {
  font-size: 0.75rem;
  padding: 3px 8px;
  border-radius: 12px;
  font-weight: 500;
}

.wf-status.online { color: var(--accent-green); background: rgba(63,185,80,0.1); }
.wf-status.idle { color: var(--accent-yellow); background: rgba(210,153,34,0.1); }
.wf-status.offline { color: var(--text-muted); background: rgba(110,122,138,0.1); }

.quick-start { margin-top: 8px; }

.template-list {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}

.template-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 16px 12px;
  text-align: center;
  cursor: pointer;
  transition: border-color 0.15s;
}

.template-card:hover { border-color: var(--accent-blue); }
.template-card:active { transform: scale(0.97); }

.tpl-icon { font-size: 1.5rem; display: block; margin-bottom: 6px; }
.tpl-label { font-size: 0.8rem; color: var(--text-secondary); }
</style>
