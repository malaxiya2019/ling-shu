#!/usr/bin/env bash
# Lingshu HTTP Load Test
# 启动服务后用 wrk 打压测 ratelimit + billing 模块
set -euo pipefail

BASE_URL="${1:-http://localhost:8080}"
DURATION="${2:-30}"
CONNECTIONS="${3:-10}"
THREADS="${4:-2}"

echo "=========================================="
echo "  Lingshu HTTP Load Test"
echo "  Target: $BASE_URL"
echo "  Duration: ${DURATION}s"
echo "  Connections: $CONNECTIONS"
echo "  Threads: $THREADS"
echo "=========================================="

# 1. 健康检查基准
echo ""
echo "--- 1/4: Health Check (GET /health) ---"
wrk -t"$THREADS" -c"$CONNECTIONS" -d"${DURATION}s" \
  --latency \
  "$BASE_URL/health"

# 2. 列出模型
echo ""
echo "--- 2/4: List Models (GET /v1/models) ---"
wrk -t"$THREADS" -c"$CONNECTIONS" -d"${DURATION}s" \
  --latency \
  "$BASE_URL/v1/models"

# 3. Chat completions (non-streaming, mock LLM)
echo ""
echo "--- 3/4: Chat Completions (POST /v1/chat/completions) ---"
PAYLOAD='{"model":"mock","messages":[{"role":"user","content":"hello"}],"stream":false}'
wrk -t"$THREADS" -c"$CONNECTIONS" -d"${DURATION}s" \
  --latency \
  --header "Content-Type: application/json" \
  --body "$PAYLOAD" \
  -s <(echo 'wrk.headers["Content-Type"] = "application/json"') \
  "$BASE_URL/v1/chat/completions"

# 4. 高并发短请求（压 ratelimit）
echo ""
echo "--- 4/4: High Concurrency Burst (GET /health) ---"
wrk -t4 -c64 -d"10s" --latency "$BASE_URL/health"

echo ""
echo "=========================================="
echo "  Load test completed!"
echo "=========================================="
