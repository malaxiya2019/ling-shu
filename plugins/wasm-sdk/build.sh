#!/usr/bin/env bash
# LingShu WASM Plugin SDK — 构建脚本
set -euo pipefail

TARGET="${1:-wasm32-wasip1}"
RELEASE="${2:-release}"

echo "=== Building WASM Plugin SDK (target: $TARGET, profile: $RELEASE) ==="

if ! rustup target list --installed | grep -q "$TARGET"; then
    echo "Adding target $TARGET..."
    rustup target add "$TARGET"
fi

BUILD_FLAGS=""
if [ "$RELEASE" = "release" ]; then
    BUILD_FLAGS="--release"
fi

cargo build $BUILD_FLAGS --target "$TARGET"

echo "=== Build complete ==="
echo "Output: target/$TARGET/${RELEASE}/lingshu_wasm_plugin.wasm"
