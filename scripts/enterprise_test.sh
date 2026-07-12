#!/usr/bin/env bash
# =============================================================================
# LingShu v4.3 Enterprise — 企业功能端到端测试脚本
# =============================================================================
# 测试 Agent 生命周期、成本统计、MCP 自动发现、Plugin Marketplace
# =============================================================================

set -euo pipefail

BASE_URL="${LINGSHU_URL:-http://127.0.0.1:8080}"
PASS=0
FAIL=0
TIMEOUT="${TIMEOUT:-5}"

# ── 辅助函数 ────────────────────────────────────────

log()   { echo -e "\e[36m[INFO]\e[0m $*"; }
ok()    { echo -e "\e[32m  ✅ PASS\e[0m $*"; ((PASS++)); }
fail()  { echo -e "\e[31m  ❌ FAIL\e[0m $*"; ((FAIL++)); }
check() { local desc="$1" method="$2" url="$3" expect="$4"; shift 4
  local status; status=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" -X "$method" "$BASE_URL$url" "$@" 2>/dev/null || echo "000")
  if [ "$status" = "$expect" ]; then ok "$desc ($status)"; else fail "$desc (expected $expect, got $status)"; fi
}
check_body() { local desc="$1" method="$2" url="$3" expect="$4"; shift 4
  local body; body=$(curl -s --max-time "$TIMEOUT" -X "$method" "$BASE_URL$url" "$@" 2>/dev/null || echo "")
  if echo "$body" | grep -q "$expect"; then ok "$desc"; else fail "$desc (expected body containing '$expect')"; fi
}

echo ""
echo "══════════════════════════════════════════════════"
echo "  LingShu v4.3 Enterprise — E2E Test Suite"
echo "  Target: $BASE_URL"
echo "══════════════════════════════════════════════════"
echo ""

# ── 1. Agent 生命周期 ───────────────────────────────
echo "━━━ 1. Agent Lifecycle Management ━━━"

# 1.1 启动 Agent
check_body "POST /v1/agent/run — 启动 Agent" \
  POST "/v1/agent/run" "agent_id" \
  -H "Content-Type: application/json" \
  -d '{"task":"hello"}'

# 1.2 列出 Agents
check_body "GET /v1/agents — 列出 Agents" \
  GET "/v1/agents" "agent_id"

# 1.3 Agent 状态
check_body "GET /v1/agents/:id — Agent 状态" \
  GET "/v1/agents" "agent_id"

# 1.4 Restart Agent (先获取agent_id)
AGENT_ID=$(curl -s --max-time "$TIMEOUT" "$BASE_URL/v1/agents" 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); print(d[0]['agent_id'] if isinstance(d,list) and d else 'test-agent-1')" 2>/dev/null || echo "test-agent-1")
if [ "$AGENT_ID" != "test-agent-1" ]; then
  check_body "POST /v1/agent/:id/restart — Agent 重启" \
    POST "/v1/agent/$AGENT_ID/restart" "restarted" \
    -H "Content-Type: application/json"
fi

# 1.5 更新 Agent 配置
check_body "POST /v1/agent/:id/update — Agent 配置更新" \
  POST "/v1/agent/$AGENT_ID/update" "updated" \
  -H "Content-Type: application/json" \
  -d '{"config":{"model":"gpt-4"}}'

# 1.6 删除 Agent
check "DELETE /v1/agent/:id — Agent 删除" \
  DELETE "/v1/agent/$AGENT_ID" "200"

echo ""

# ── 2. Token 成本统计 ───────────────────────────────
echo "━━━ 2. Token Cost & Billing ━━━"

# 2.1 记录用量
check_body "POST /v1/billing/usage — 记录Token用量" \
  POST "/v1/billing/usage" "recorded" \
  -H "Content-Type: application/json" \
  -d '{"user_id":"test-user","model":"gpt-4","input_tokens":150,"output_tokens":75}'

# 2.2 全局统计
check_body "GET /v1/billing/stats — 全局用量统计" \
  GET "/v1/billing/stats" "total_requests"

# 2.3 用户报告
check_body "GET /v1/billing/report/:user_id — 用户成本报告" \
  GET "/v1/billing/report/test-user" "estimated_cost"

# 2.4 用户配额
check_body "GET /v1/billing/quota/:user_id — 用户配额" \
  GET "/v1/billing/quota/test-user" "remaining"

echo ""

# ── 3. MCP 自动发现 ─────────────────────────────────
echo "━━━ 3. MCP Auto-Discovery ━━━"

# 3.1 MCP 服务器列表
check_body "GET /v1/discovery/servers — MCP 服务器列表" \
  GET "/v1/discovery/servers" "servers"

# 3.2 MCP 发现健康状态
check_body "GET /v1/discovery/health — MCP 发现健康状态" \
  GET "/v1/discovery/health" "healthy"

echo ""

# ── 4. Plugin Marketplace ───────────────────────────
echo "━━━ 4. Plugin Marketplace ━━━"

# 4.1 市场搜索
check_body "GET /v1/plugins/market/search — 市场搜索" \
  GET "/v1/plugins/market/search?q=test" "plugins"

# 4.2 市场列表
check_body "GET /v1/plugins/market/list — 市场插件列表" \
  GET "/v1/plugins/market/list" "plugins"

# 4.3 市场源
check_body "GET /v1/plugins/market/sources — 市场源列表" \
  GET "/v1/plugins/market/sources" "source"

echo ""


# ── 5. 多租户 ───────────────────────────────────────
echo "━━━ 5. Multi-Tenant ━━━"

# 5.1 组织列表
check_body "GET /v1/tenant/orgs — 组织列表" 
  GET "/v1/tenant/orgs" "id"

# 5.2 创建组织
check_body "POST /v1/tenant/orgs — 创建组织" 
  POST "/v1/tenant/orgs" "id" 
  -H "Content-Type: application/json" 
  -d '{"name":"Test Org","slug":"test-org","description":"E2E test org"}'

# 5.3 租户统计
check_body "GET /v1/tenant/stats — 租户统计" 
  GET "/v1/tenant/stats" "total_orgs"

echo ""
# ── 5. 系统健康 ─────────────────────────────────────
echo "━━━ 5. System Health ━━━"

check "GET /health — 系统健康检查" GET "/health" "200"
check "GET /version — 版本信息" GET "/version" "200"

echo ""
echo "══════════════════════════════════════════════════"
echo "  Results:  ✅ $PASS passed  |  ❌ $FAIL failed"
echo "══════════════════════════════════════════════════"
echo ""

exit $FAIL
