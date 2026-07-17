// ════════════════════════════════════════════════════════════
// 灵枢 v5 — 合并服务器（API + 静态文件，同端口）
// 启动: node server/combined-server.mjs
// 手机浏览器打开: http://<本机IP>:8080
// ════════════════════════════════════════════════════════════

import http from 'node:http'
import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const DIST = path.join(__dirname, '..', 'dist')
const PORT = 8080
const startTime = Date.now()

// MIME types
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css',
  '.js': 'application/javascript',
  '.json': 'application/json',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.map': 'application/json',
}

function uptime() {
  const s = Math.floor((Date.now() - startTime) / 1000)
  const d = Math.floor(s / 86400)
  const h = Math.floor((s % 86400) / 3600)
  const m = Math.floor((s % 3600) / 60)
  return `${d}d ${h}h ${m}m`
}

// ── Mock Data ──

const health = {
  status: 'healthy',
  version: '5.0.0-dev',
  uptime: '0d 0h 0m',
  checks: [
    { name: 'runtime',   healthy: true,  detail: '3 active agents' },
    { name: 'plugins',   healthy: true,  detail: '12 plugins registered' },
    { name: 'eventbus',  healthy: true,  detail: 'operational' },
    { name: 'memory',    healthy: true,  detail: 'operational' },
    { name: 'mcp',       healthy: true,  detail: '5 servers connected' },
    { name: 'websocket', healthy: true,  detail: '2 connections' },
  ]
}

const versionInfo = {
  version: '5.0.0-dev',
  build_date: '2026-07-17',
  commit: 'a1b2c3d4e5f6',
}

const agents = {
  agents: [
    { agent_id: 'ag_8a3f2b1c', name: '代码助手',    status: 'Running', created_at: '2026-07-16' },
    { agent_id: 'ag_f7e2d4c5', name: '数据分析师',  status: 'Running', created_at: '2026-07-15' },
    { agent_id: 'ag_3b9c1a7d', name: '文档助手',    status: 'Idle',    created_at: '2026-07-14' },
    { agent_id: 'ag_6d4e8f2a', name: '翻译助手',    status: 'Stopped', created_at: '2026-07-13' },
    { agent_id: 'ag_1c5a9b3e', name: '客服机器人',  status: 'Running', created_at: '2026-07-12' },
    { agent_id: 'ag_9e2f7b4d', name: '邮件助手',    status: 'Idle',    created_at: '2026-07-11' },
  ]
}

const plugins = {
  plugins: [
    { id: 'pl_001', name: 'web-search',   version: '1.2.0', status: 'running',  author: 'lingshu' },
    { id: 'pl_002', name: 'code-sandbox', version: '2.0.1', status: 'running',  author: 'lingshu' },
    { id: 'pl_003', name: 'rag-plugin',   version: '1.5.0', status: 'running',  author: 'lingshu' },
    { id: 'pl_004', name: 'scheduler',    version: '0.9.0', status: 'active',   author: 'lingshu' },
    { id: 'pl_005', name: 'beef',         version: '1.0.0', status: 'stopped',  author: 'lingshu' },
  ],
  total: 5,
}

const federationStatus = {
  cluster_id: 'cls_lingshu_default',
  cluster_name: '默认集群',
  enabled: true,
  node_count: 0,
  uptime_secs: 300,
}

const federationNodes = []

const models = [
  { id: 'deepseek-v4',    name: 'DeepSeek V4 Pro',  provider: 'deepseek', capabilities: ['chat', 'code', 'reasoning'] },
  { id: 'gpt-4o',         name: 'GPT-4o',           provider: 'openai',   capabilities: ['chat', 'vision', 'code'] },
  { id: 'claude-3.5',     name: 'Claude 3.5 Sonnet', provider: 'anthropic', capabilities: ['chat', 'code', 'reasoning'] },
]

// ── Helpers ──

function sendJSON(res, statusCode, data) {
  res.writeHead(statusCode, {
    'Content-Type': 'application/json',
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
  })
  res.end(JSON.stringify(data))
}

function serveStatic(urlPath, res) {
  // Normalize path, prevent directory traversal
  let filePath = path.normalize(path.join(DIST, urlPath))
  if (!filePath.startsWith(DIST)) {
    filePath = path.join(DIST, 'index.html')
  }

  // Default to index.html for directories
  try {
    if (fs.statSync(filePath).isDirectory()) {
      filePath = path.join(filePath, 'index.html')
    }
  } catch {
    filePath = path.join(DIST, 'index.html')
  }

  // If file doesn't exist, serve index.html (SPA fallback)
  if (!fs.existsSync(filePath)) {
    filePath = path.join(DIST, 'index.html')
  }

  try {
    const ext = path.extname(filePath).toLowerCase()
    const contentType = MIME[ext] || 'application/octet-stream'
    
    const content = fs.readFileSync(filePath)
    res.writeHead(200, {
      'Content-Type': contentType,
      'Cache-Control': ext === '.js' || ext === '.css' ? 'max-age=31536000, immutable' : 'no-cache',
    })
    res.end(content)
    return true
  } catch {
    return false
  }
}

// ── Server ──

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`)
  const pathname = url.pathname
  const method = req.method

  // CORS preflight
  if (method === 'OPTIONS') {
    res.writeHead(204, {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type',
    })
    res.end()
    return
  }

  // Update dynamic data
  health.uptime = uptime()
  federationStatus.uptime_secs = Math.floor((Date.now() - startTime) / 1000)

  // ── API Routes ──
  if (method === 'GET' && pathname === '/health') {
    sendJSON(res, 200, health)
  }
  else if (method === 'GET' && pathname === '/version') {
    sendJSON(res, 200, versionInfo)
  }
  else if (method === 'GET' && pathname === '/v1/models') {
    sendJSON(res, 200, models)
  }
  else if (method === 'GET' && pathname === '/v1/agents') {
    sendJSON(res, 200, agents)
  }
  else if (method === 'GET' && pathname === '/v1/plugins') {
    sendJSON(res, 200, plugins)
  }
  else if (method === 'GET' && pathname === '/v1/federation/status') {
    sendJSON(res, 200, federationStatus)
  }
  else if (method === 'GET' && pathname === '/v1/federation/nodes') {
    sendJSON(res, 200, federationNodes)
  }
  else if (method === 'GET' && pathname === '/v1/metrics') {
    sendJSON(res, 200, {
      timestamp: new Date().toISOString(),
      cpu_percent: 12.5,
      memory_mb: 256,
      requests_per_sec: 3.2,
      avg_latency_ms: 145,
    })
  }
  else if (method === 'GET' && pathname === '/v1/mcp/tools') {
    sendJSON(res, 200, [
      { name: 'web_search', description: '搜索网络信息', server: 'web-search' },
      { name: 'execute_code', description: '执行代码片段', server: 'code-sandbox' },
      { name: 'read_document', description: '读取文档内容', server: 'rag-plugin' },
    ])
  }
  else if (method === 'POST' && pathname === '/v1/chat') {
    let body = ''
    req.on('data', chunk => body += chunk)
    req.on('end', () => {
      try {
        const reqData = JSON.parse(body)
        const userMsg = reqData.messages?.[reqData.messages.length - 1]?.content || ''
        
        let reply = ''
        if (userMsg.includes('Agent') || userMsg.includes('agent') || userMsg.includes('智能体')) {
          reply = 'AI Agent（智能体）是一种能够自主感知环境、做出决策并执行行动的 AI 程序。它不同于传统的聊天机器人，Agent 可以：\n\n1. **自主决策** — 根据目标选择最佳行动路径\n2. **使用工具** — 调用搜索引擎、代码执行器等外部工具\n3. **长期记忆** — 记住历史对话和用户偏好\n4. **多步骤规划** — 分解复杂任务并逐步执行\n\n灵枢平台目前支持创建和管理多种类型的 Agent，你可以试试看！'
        } else if (userMsg.includes('Python') || userMsg.includes('python') || userMsg.includes('排序')) {
          reply = '```python\ndef quick_sort(arr):\n    if len(arr) <= 1:\n        return arr\n    pivot = arr[len(arr) // 2]\n    left = [x for x in arr if x < pivot]\n    middle = [x for x in arr if x == pivot]\n    right = [x for x in arr if x > pivot]\n    return quick_sort(left) + middle + quick_sort(right)\n\n# 使用示例\nnums = [3, 6, 8, 10, 1, 2, 1]\nprint(quick_sort(nums))\n# 输出: [1, 1, 2, 3, 6, 8, 10]\n```\n这是一个经典的快速排序实现，时间复杂度 O(n log n)。'
        } else {
          reply = '你好！我是灵枢 AI 助手 🇨🇳\n\n我收到你的消息了：「' + userMsg.slice(0, 50) + (userMsg.length > 50 ? '…' : '') + '」\n\n我可以帮你：\n- 🤖 创建和管理 AI 智能体\n- 💻 编写和调试代码\n- 📊 分析数据和生成报告\n- 🔍 搜索和整理信息\n- 📝 文档撰写和翻译\n\n请问有什么具体需要我帮忙的吗？'
        }

        sendJSON(res, 200, {
          message: {
            role: 'assistant',
            content: reply,
          },
          session_id: 'session_' + Date.now().toString(36),
        })
      } catch (e) {
        sendJSON(res, 400, { error: 'invalid request' })
      }
    })
  }
  else if (method === 'GET' && pathname === '/docs') {
    sendJSON(res, 200, { models })
  }
  else {
    // ── Static Files (with SPA fallback) ──
    const served = serveStatic(pathname, res)
    if (!served) {
      sendJSON(res, 404, { error: 'not found', path: pathname })
    }
  }

  // Log only non-static requests to reduce noise
  if (pathname.startsWith('/health') || pathname.startsWith('/version') || pathname.startsWith('/v1/')) {
    const timestamp = new Date().toLocaleTimeString('zh-CN', { hour12: false })
    console.log(`  ${timestamp} ${method} ${pathname}`)
  }
})

server.listen(PORT, '0.0.0.0', () => {
  console.log('')
  console.log('╔════════════════════════════════════════════════════╗')
  console.log('║   🚀 灵枢 AI 平台 v5                              ║')
  console.log('║  合并服务器（API + 前端静态文件）                    ║')
  console.log('╠════════════════════════════════════════════════════╣')
  console.log(`║  监听:  http://0.0.0.0:${PORT}`)
  console.log('╠════════════════════════════════════════════════════╣')
  console.log('║  API Endpoints:                                    ║')
  console.log('║  GET  /health       — 系统健康检查                   ║')
  console.log('║  GET  /version      — 版本信息                       ║')
  console.log('║  GET  /v1/agents    — 智能体列表                     ║')
  console.log('║  GET  /v1/plugins   — 插件列表                       ║')
  console.log('║  GET  /v1/models    — 模型列表                       ║')
  console.log('║  GET  /v1/federation/* — 联邦网络                    ║')
  console.log('║  GET  /v1/metrics   — 系统指标                       ║')
  console.log('║  GET  /v1/mcp/tools — MCP 工具列表                   ║')
  console.log('║  POST /v1/chat     — AI 对话                        ║')
  console.log('╚════════════════════════════════════════════════════╝')
  console.log('')
  
  // Show local IPs for mobile access
  const ifaces = os.networkInterfaces()
  for (const name of Object.keys(ifaces)) {
    for (const iface of ifaces[name]) {
      if (iface.family === 'IPv4' && !iface.internal) {
        console.log(`  📱 手机访问: http://${iface.address}:${PORT}`)
      }
    }
  }
  if (os.networkInterfaces()['lo']) {
    console.log(`  📱 本机:      http://127.0.0.1:${PORT}`)
  }
  
  console.log('')
  console.log(`  ${new Date().toLocaleString('zh-CN')} 已启动`)
  console.log('')
})
