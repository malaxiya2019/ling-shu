<template>
  <div class="page-padding">
    <header class="page-header">
      <button class="back-btn" @click="$router.back()">‹ 返回</button>
      <h1 class="page-title">🔥 热重载</h1>
    </header>
    <div class="empty-state">
      <div class="empty-icon">🔥</div>
      <h2>热重载</h2>
      <p>插件与配置热重载管理</p>
    </div>
  </div>
</template>
<style scoped>
.page-header { display:flex; align-items:center; gap:12px; margin-bottom:20px; padding-top:8px; }
.back-btn { background:none; border:none; color:var(--accent-blue); font-size:.95rem; cursor:pointer; font-family:inherit; }
.page-title { font-size:1.1rem; font-weight:700; flex:1; }
.empty-state { display:flex; flex-direction:column; align-items:center; padding:60px 0; gap:8px; color:var(--text-muted); }
.empty-icon { font-size:3rem; }
.empty-state h2 { font-size:1.1rem; color:var(--text-secondary); }
.empty-state p { font-size:.85rem; }
</style>
