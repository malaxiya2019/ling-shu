import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'home',
      component: () => import('@/views/Home.vue'),
      meta: { title: '首页', icon: '🏠' },
    },
    {
      path: '/chat',
      name: 'chat',
      component: () => import('@/views/Chat.vue'),
      meta: { title: 'AI 助手', icon: '💬' },
    },
    {
      path: '/agents',
      name: 'agents',
      component: () => import('@/views/Agents.vue'),
      meta: { title: '智能体', icon: '🤖' },
    },
    {
      path: '/workflows',
      name: 'workflows',
      component: () => import('@/views/Workflows.vue'),
      meta: { title: '工作流', icon: '🔄' },
    },
    {
      path: '/more',
      name: 'more',
      component: () => import('@/views/More.vue'),
      meta: { title: '更多', icon: '📋' },
    },
    // Developer tools (nested under more functionally, but has own routes)
    {
      path: '/knowledge',
      name: 'knowledge',
      component: () => import('@/views/Knowledge.vue'),
      meta: { title: '知识库', icon: '📁', category: 'secondary' },
    },
    {
      path: '/mcp',
      name: 'mcp',
      component: () => import('@/views/McpServices.vue'),
      meta: { title: 'MCP 服务', icon: '🔌', category: 'secondary' },
    },
    {
      path: '/federation',
      name: 'federation',
      component: () => import('@/views/Federation.vue'),
      meta: { title: '联邦网络', icon: '🌐', category: 'secondary' },
    },
    {
      path: '/monitor',
      name: 'monitor',
      component: () => import('@/views/Monitor.vue'),
      meta: { title: '系统监控', icon: '📊', category: 'secondary' },
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('@/views/Settings.vue'),
      meta: { title: '系统设置', icon: '⚙️', category: 'secondary' },
    },
    // Developer tools
    {
      path: '/dev/api-docs',
      name: 'api-docs',
      component: () => import('@/views/dev/ApiDocs.vue'),
      meta: { title: '接口文档', icon: '📄', category: 'developer' },
    },
    {
      path: '/dev/logs',
      name: 'logs',
      component: () => import('@/views/dev/Logs.vue'),
      meta: { title: '系统日志', icon: '📝', category: 'developer' },
    },
    {
      path: '/dev/hot-reload',
      name: 'hot-reload',
      component: () => import('@/views/dev/HotReload.vue'),
      meta: { title: '热重载', icon: '🔥', category: 'developer' },
    },
    {
      path: '/dev/debug',
      name: 'debug',
      component: () => import('@/views/dev/DebugTools.vue'),
      meta: { title: '调试工具', icon: '🧪', category: 'developer' },
    },
  ],
})

export default router
