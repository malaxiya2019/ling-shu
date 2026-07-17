#!/bin/bash
# 检查 app/Cargo.toml feature 传播一致性
# 确保所有可选功能 feature 都正确传播到下游 crate

set -euo pipefail

cd "$(git rev-parse --show-toplevel)" 2>/dev/null || cd "$(dirname "$0")/.."

errors=0
warnings=0

echo "🔍 检查 app/Cargo.toml feature 传播一致性..."
echo ""

# 已知应该为非空的 feature（这些是元 feature 或 crate-internal feature）
known_meta_features=("default" "swarm" "autonomy" "telegram" "feishu" "qq" "wechat" "discord" "audit-sqlite")

is_known_meta() {
    local f="$1"
    for k in "${known_meta_features[@]}"; do
        [[ "$f" == "$k" ]] && return 0
    done
    return 1
}

while IFS='=' read -r line; do
    feature=$(echo "$line" | cut -d= -f1 | xargs)
    deps=$(echo "$line" | cut -d= -f2- | xargs)
    
    # 跳过非 feature 行
    [[ -z "$feature" ]] && continue
    [[ "$feature" =~ ^[a-z] ]] || continue
    is_known_meta "$feature" && continue
    
    if [[ "$deps" == "[]" ]]; then
        echo "❌ 空 feature: $feature = []"
        echo "   这个 feature 没有传播到下游 crate！"
        echo "   例如: $feature = [\"lingshu-orchestrator/$feature\"]"
        ((errors++))
    fi
done < <(grep -E "^[a-z].* = " app/Cargo.toml)

echo ""
if [[ $errors -gt 0 ]]; then
    echo "❌ 发现 $errors 个未传播的 feature"
    exit 1
else
    echo "✅ 所有 feature 传播正确"
fi
