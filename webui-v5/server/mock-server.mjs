// ════════════════════════════════════════════════════════════
// 灵枢 v5 — 模拟后端 API 服务器
// 用于前端开发联调，模拟 Rust 后端的 API 响应
// ════════════════════════════════════════════════════════════
// 启动: node server/mock-server.mjs
// 监听的端口会被前端 Vite proxy 代理

import http from 'node:http'

const PORT = 8080
const startTime = Date.now()

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
  uptime: '0d 0h 5m',
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

const modelsHandler = `
Available models:
- deepseek-v4 / DeepSeek V4 Pro
- gpt-4o      / GPT-4o
- claude-3.5  / Claude 3.5 Sonnet
`

// ── Request Router ──

function sendJSON(res, statusCode, data) {
  const json = JSON.stringify(data, null, 2)
  res.writeHead(statusCode, {
    'Content-Type': 'application/json',
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
  })
  res.end(json)
}

function sendText(res, statusCode, text) {
  res.writeHead(statusCode, {
    'Content-Type': 'text/plain; charset=utf-8',
    'Access-Control-Allow-Origin': '*',
  })
  res.end(text)
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`)
  const path = url.pathname
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

  // Update uptime dynamically
  health.uptime = uptime()
  federationStatus.uptime_secs = Math.floor((Date.now() - startTime) / 1000)
  federationStatus.uptime = uptime()

  // ── Route matching ──
  if (method === 'GET' && path === '/health') {
    sendJSON(res, 200, health)
  }
  else if (method === 'GET' && path === '/version') {
    sendJSON(res, 200, versionInfo)
  }
  else if (method === 'GET' && path === '/v1/models') {
    sendJSON(res, 200, models)
  }
  else if (method === 'GET' && path === '/v1/agents') {
    sendJSON(res, 200, agents)
  }
  else if (method === 'GET' && path === '/v1/plugins') {
    sendJSON(res, 200, plugins)
  }
  else if (method === 'GET' && path === '/v1/federation/status') {
    sendJSON(res, 200, federationStatus)
  }
  else if (method === 'GET' && path === '/v1/federation/nodes') {
    sendJSON(res, 200, federationNodes)
  }
  else if (method === 'GET' && path === '/v1/metrics') {
    sendJSON(res, 200, {
      timestamp: new Date().toISOString(),
      cpu_percent: 12.5,
      memory_mb: 256,
      requests_per_sec: 3.2,
      avg_latency_ms: 145,
    })
  }
  else if (method === 'GET' && path === '/v1/mcp/tools') {
    sendJSON(res, 200, [
      { name: 'web_search', description: '搜索网络信息', server: 'web-search' },
      { name: 'execute_code', description: '执行代码片段', server: 'code-sandbox' },
      { name: 'read_document', description: '读取文档内容', server: 'rag-plugin' },
    ])
  }
  else if (method === 'POST' && path === '/v1/chat') {
    // 模拟 AI 回复
    let body = ''
    req.on('data', chunk => body += chunk)
    req.on('end', () => {
      try {
        const reqData = JSON.parse(body)
        const userMsg = reqData.messages?.[reqData.messages.length - 1]?.content || ''
        
        // 简单的关键词回复逻辑
        let reply = ''
        if (userMsg.includes('Agent') || userMsg.includes('agent') || userMsg.includes('智能体')) {
          reply = 'AI Agent（智能体）是一种能够自主感知环境、做出决策并执行行动的 AI 程序。它不同于传统的聊天机器人，Agent 可以：\n\n1. **自主决策** — 根据目标选择最佳行动路径\n2. **使用工具** — 调用搜索引擎、代码执行器等外部工具\n3. **长期记忆** — 记住历史对话和用户偏好\n4. **多步骤规划** — 分解复杂任务并逐步执行\n\n灵枢平台目前支持创建和管理多种类型的 Agent，你可以试试看！'
        } else if (userMsg.includes('Python') || userMsg.includes('python') || userMsg.includes('排序')) {
          reply = '```python\ndef quick_sort(arr):\n    if len(arr) <= 1:\n        return arr\n    pivot = arr[len(arr) // 2]\n    left = [x for x in arr if x < pivot]\n    middle = [x for x in arr if x == pivot]\n    right = [x for x in arr if x > pivot]\n    return quick_sort(left) + middle + quick_sort(right)\n\n# 使用示例\nnums = [3, 6, 8, 10, 1, 2, 1]\nprint(quick_sort(nums))\n# 输出: [1, 1, 2, 3, 6, 8, 10]\n```\n这是一个经典的快速排序实现，时间复杂度 O(n log n)。'
        } else {
          reply = `你好！我是灵枢 AI 助手 🇨🇳\n\n我收到你的消息了：「${userMsg.slice(0, 50)}${userMsg.length > 50 ? '…' : ''}」\n\n我可以帮你：\n- 🤖 创建和管理 AI 智能体\n- 💻 编写和调试代码\n- 📊 分析数据和生成报告\n- 🔍 搜索和整理信息\n- 📝 文档撰写和翻译\n\n请问有什么具体需要我帮忙的吗？`
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
  else if (method === 'GET' && path === '/docs') {
    sendText(res, 200, modelsHandler)
  }
  else {
    // 未匹配的路由返回 404
    sendJSON(res, 404, { error: 'not found', path })
  }

  // Log request
  const timestamp = new Date().toLocaleTimeString('zh-CN', { hour12: false })
  console.log(`  ${timestamp} ${method} ${path}`)
})

server.listen(PORT, '0.0.0.0', () => {
  console.log('')
  console.log('╔══════════════════════════════════════════╗')
  console.log('║  灵枢 v5 模拟 API 服务器                 ║')
  console.log('╠══════════════════════════════════════════╣')
  console.log(`║  监听:  http://0.0.0.0:${PORT}                    ║`)
  console.log('║  用途:  前端开发联调                      ║')
  console.log('╚══════════════════════════════════════════╝')
  console.log('')
  console.log('  API Endpoints:')
  console.log(`  GET  /health`)
  console.log(`  GET  /version`)
  console.log(`  GET  /v1/agents`)
  console.log(`  GET  /v1/plugins`)
  console.log(`  GET  /v1/models`)
  console.log(`  GET  /v1/federation/status`)
  console.log(`  GET  /v1/federation/nodes`)
  console.log(`  GET  /v1/metrics`)
  console.log(`  GET  /v1/mcp/tools`)
  console.log(`  POST /v1/chat`)
  console.log('')
  console.log(`  ${new Date().toLocaleString('zh-CN')} 已启动`)
  console.log('')
})
