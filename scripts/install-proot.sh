#!/usr/bin/env bash
set -euo pipefail

# Lingshu 安装脚本 - Termux proot Ubuntu 专用
# 一行执行: bash /data/data/com.termux/files/home/ling-shu/scripts/install-proot.sh

LINGSHU_DIR="/data/data/com.termux/files/home/ling-shu"

echo "=== 1. 安装 Linux 原生 Rust (aarch64-unknown-linux-gnu) ==="
if ! command -v rustc &>/dev/null; then
    cd /tmp
    curl -fsSL https://sh.rustup.rs | sh -s -- -y --default-host aarch64-unknown-linux-gnu 2>&1
    . "$HOME/.cargo/env"
fi

echo ""
echo "=== 2. 编译 Lingshu ==="
cd "$LINGSHU_DIR"
cargo build --release -p lingshu

echo ""
echo "=== 3. 安装二进制 ==="
cp target/release/lingshu /usr/local/bin/lingshu
echo "Installed: /usr/local/bin/lingshu ($(ls -lh /usr/local/bin/lingshu | awk '{print $5}'))"

echo ""
echo "=== 4. 配置 ==="
mkdir -p ~/.local/share/lingshu ~/.config/lingshu
if [ ! -f ~/.config/lingshu/.env ]; then
    cp .env.example ~/.config/lingshu/.env
    sed -i "s|^LS_STORAGE_DIR=.*|LS_STORAGE_DIR=$HOME/.local/share/lingshu/storage|" ~/.config/lingshu/.env
    sed -i "s|^LS_DATABASE_URL=.*|LS_DATABASE_URL=$HOME/.local/share/lingshu/lingshu.db|" ~/.config/lingshu/.env
fi

echo ""
echo "=== 完成 ==="
echo "运行: lingshu"
