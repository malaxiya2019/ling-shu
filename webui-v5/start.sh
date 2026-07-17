#!/bin/bash
# 灵枢 v5 开发服务器启动脚本
echo "🚀 启动灵枢 AI 平台 v5..."
echo ""
echo "📱 手机端访问: http://localhost:5173"
echo "💻 局域网访问: http://$(hostname -I 2>/dev/null | awk '{print $1}'):5173"
echo ""
npx vite --host
