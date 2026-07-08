#!/usr/bin/env bash
# LingShu Plugin Marketplace — 安装脚本
set -euo pipefail

MARKET_DIR="$(cd "$(dirname "$0")" && pwd)"
LINGSHU_HOME="${LINGSHU_HOME:-${HOME}/.lingshu}"
PLUGIN_DIR="${LINGSHU_HOME}/plugins"

usage() {
    echo "Usage: $0 <plugin-name>"
    echo ""
    echo "Available plugins:"
    jq -r '.plugins[] | "  \(.name) — \(.description)"' "$MARKET_DIR/index.json"
    exit 1
}

if [ $# -lt 1 ]; then
    usage
fi

PLUGIN_NAME="$1"

# 从 index 查找插件
PLUGIN_ENTRY=$(jq -r --arg name "$PLUGIN_NAME" '.plugins[] | select(.name == $name)' "$MARKET_DIR/index.json")
if [ -z "$PLUGIN_ENTRY" ]; then
    echo "Error: plugin '$PLUGIN_NAME' not found in marketplace"
    usage
fi

# 创建插件目录
mkdir -p "$PLUGIN_DIR/$PLUGIN_NAME"

# 复制插件元信息
echo "$PLUGIN_ENTRY" | jq '.' > "$PLUGIN_DIR/$PLUGIN_NAME/plugin.json"

# 如果是本地插件，复制二进制
RUNTIME=$(echo "$PLUGIN_ENTRY" | jq -r '.runtime')
ENTRY=$(echo "$PLUGIN_ENTRY" | jq -r '.entry')

if [ -f "$MARKET_DIR/../../$ENTRY" ]; then
    cp "$MARKET_DIR/../../$ENTRY" "$PLUGIN_DIR/$PLUGIN_NAME/"
    echo "Copied plugin binary to $PLUGIN_DIR/$PLUGIN_NAME/"
fi

echo "Plugin '$PLUGIN_NAME' installed successfully!"
echo "Location: $PLUGIN_DIR/$PLUGIN_NAME/"
