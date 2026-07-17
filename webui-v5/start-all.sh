#!/bin/bash
# 灵枢 v5 — 一键启动（模拟后端 + 前端开发服务器）
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║     🚀 灵枢 AI 平台 v5                   ║"
echo "║     启动中...                            ║"
echo "╚══════════════════════════════════════════╝"
echo ""

# 1. 启动模拟 API 服务器（后台）
echo "[1/2] 启动模拟 API 服务器..."
node "$SCRIPT_DIR/server/mock-server.mjs" &
MOCK_PID=$!
echo "  PID: $MOCK_PID"
sleep 1

# 验证 mock server 启动成功
if ! curl -sf http://localhost:8080/health > /dev/null 2>&1; then
  echo "  ❌ 模拟服务器启动失败"
  exit 1
fi
echo "  ✅ http://localhost:8080/health OK"

echo ""

# 2. 启动 Vite 开发服务器
echo "[2/2] 启动前端开发服务器..."
echo ""
cd "$SCRIPT_DIR"
npx vite --host &
VITE_PID=$!

# 捕获退出信号，清理子进程
cleanup() {
  echo ""
  echo "🛑 关闭服务..."
  kill $VITE_PID 2>/dev/null
  kill $MOCK_PID 2>/dev/null
  wait
  echo "已关闭"
  exit 0
}
trap cleanup SIGINT SIGTERM

# 等待任意子进程退出
wait
