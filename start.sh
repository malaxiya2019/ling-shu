#!/usr/bin/env bash
# =============================================================================
# 🚀 Lingshu (灵枢) — 一键启动脚本
# =============================================================================
# 用法:
#   ./start.sh                   首次运行（交互式配置向导）
#   ./start.sh --quick           跳过检查，直接用现有配置启动
#   ./start.sh --repl            启动 REPL 交互模式
#   ./start.sh --check-env       仅检查环境依赖，不启动
#   ./start.sh --addr 0.0.0.0:8080  自定义监听地址
#   ./start.sh --env prod        生产模式
#   ./start.sh --help            显示帮助
# =============================================================================

set -euo pipefail

# ── 颜色 ──
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'
BOLD='\033[1m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERR]${NC}   $*"; }
header(){ echo -e "\n${BOLD}━━━ $* ━━━${NC}"; }

MIN_RUST_VERSION="1.81"

# ── 帮助 ──
show_help() {
    cat << 'HELP'
用法: ./start.sh [选项]

选项:
  --quick             跳过交互式配置向导，直接用已有 .env 启动
  --repl              启动后进入交互式 REPL（而非 HTTP API 服务）
  --addr <host:port>  监听地址 (默认 127.0.0.1:8080)
  --env <env>         运行环境: dev | test | prod (默认 dev)
  --check-env         仅检查系统依赖是否满足，不启动服务
  --doctor            全面诊断: Rust / Cargo / 依赖 / API Key / 网络 / 权限 / 内存
  --update            拉取最新代码并重新编译
  --china             中国网络优化: 跳过国外站点检测，使用国内镜像
  --with-openclaw     集成 OpenClaw MCP 通道网关 (需要 Node.js)
  --help              显示此帮助

首次运行:
  直接运行 ./start.sh 会引导你完成：
    1. 检查 Rust / 系统依赖
    2. 创建 .env 配置文件
    3. 配置 LLM API Key
    4. 编译并启动服务

示例:
  ./start.sh                         首次运行（推荐）
  ./start.sh --quick                 后续快速启动
  ./start.sh --repl                  启动命令行交互模式
  ./start.sh --check-env             仅检查环境
  ./start.sh --doctor                全面诊断
  ./start.sh --update                更新到最新版
  ./start.sh --addr 0.0.0.0:8080    开放网络访问
  ./start.sh --china                 使用中国网络优化模式
  ./start.sh --with-openclaw        集成 OpenClaw 消息通道
HELP
    exit 0
}

# ── 参数解析 ──
QUICK=false
REPL=false
CHECK_ENV=false
DOCTOR=false
UPDATE=false
CHINA=false
WITH_OPENCLAW=false
ADDR="127.0.0.1:8080"
ENV="dev"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --quick)      QUICK=true; shift ;;
        --repl)       REPL=true; shift ;;
        --check-env)  CHECK_ENV=true; shift ;;
        --doctor)     DOCTOR=true; shift ;;
        --update)     UPDATE=true; shift ;;
        --china)      CHINA=true; shift ;;
        --with-openclaw) WITH_OPENCLAW=true; shift ;;
        --addr)       ADDR="$2"; shift 2 ;;
        --env)        ENV="$2"; shift 2 ;;
        --help|-h)    show_help ;;
        *)            err "未知选项: $1"; show_help ;;
    esac
done

# ── 1. 定位项目根目录 ──
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ── 中国网络优化 ──
if $CHINA; then
    info "🌐 中国网络模式已启用"
    info "   跳过国外站点连通性检测，使用国内镜像源"
    # 自动配置 Rust 国内镜像
    if [[ ! -f "$HOME/.cargo/config.toml" ]] || ! grep -q "mirrors.ustc.edu.cn" "$HOME/.cargo/config.toml" 2>/dev/null; then
        info "   自动配置 Rust 国内镜像加速 (USTC)..."
        mkdir -p "$HOME/.cargo"
        cat > "$HOME/.cargo/config.toml" << MIRROREOF
[source.crates-io]
replace-with = "ustc"

[source.ustc]
registry = "sparse+https://mirrors.ustc.edu.cn/crates.io-index/"
MIRROREOF
        info "   ✅ Rust 国内镜像配置完成 (~/.cargo/config.toml)"
    else
        info "   ✅ Rust 国内镜像已配置"
    fi

    # 设置 git 国内镜像（如果有 gitee remote）
    if git remote get-url gitee &>/dev/null; then
        info "   检测到 Gitee 远程仓库，使用 Gitee 加速"
    fi
fi

# 确保 cargo/rustc 在 PATH 中（Termux 兼容）
for _p in /data/data/com.termux/files/usr/bin "$HOME/.cargo/bin" "$HOME/.rustup/bin"; do
    if [[ -x "$_p/cargo" ]] && ! command -v cargo &>/dev/null; then
        export PATH="$_p:$PATH"
    fi
done

header "🚀 Lingshu (灵枢) Agent System"

# ── 2. 检测操作系统 ──
detect_os() {
    if [[ -d /data/data/com.termux/files/usr ]]; then
        if grep -qi ubuntu /etc/os-release 2>/dev/null; then
            echo "termux-proot"
        else
            echo "termux"
        fi
    elif [[ "$(uname -s)" == "Darwin" ]]; then
        echo "macos"
    elif grep -qi -e ubuntu -e debian /etc/os-release 2>/dev/null; then
        echo "debian"
    elif grep -qi -e fedora -e rhel -e centos /etc/os-release 2>/dev/null; then
        echo "rhel"
    elif grep -qi alpine /etc/os-release 2>/dev/null; then
        echo "alpine"
    else
        echo "unknown"
    fi
}
OS=$(detect_os)
ok "系统: ${OS} ($(uname -m))"

# ── 3. 依赖检查 ──
PASS_ALL=true

# 3a. Rust 工具链
check_rust() {
    header "📦 检查 Rust 工具链"
    if ! command -v cargo &>/dev/null; then
        err "未安装 Rust！"
        echo ""
        case "$OS" in
            termux)
                echo "  在 Termux 上安装:"
                echo "    pkg update && pkg install rust binutils"
                ;;
            termux-proot|debian|ubuntu)
                echo "  在 Ubuntu/Debian 上安装:"
                echo "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
                echo "    exec \$SHELL -l   # 重新加载 shell"
                echo "    或: apt install rustc cargo   # 系统包管理器版本"
                ;;
            macos)
                echo "  在 macOS 上安装:"
                echo "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
                echo "    或: brew install rust"
                ;;
            rhel|fedora|centos)
                echo "  在 RHEL/Fedora 上安装:"
                echo "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
                echo "    或: dnf install rust cargo"
                ;;
            alpine)
                echo "  在 Alpine 上安装:"
                echo "    apk add rust cargo"
                ;;
            *)
                echo "  通用安装:"
                echo "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
                ;;
        esac
        PASS_ALL=false
        return 1
    fi

    RUST_VER=$(rustc --version 2>/dev/null | grep -oP '\d+\.\d+' | head -1)
    CARGO_VER=$(cargo --version 2>/dev/null | grep -oP '\d+\.\d+' | head -1)
    ok "rustc: $(rustc --version 2>/dev/null || echo '?')"
    ok "cargo: $(cargo --version 2>/dev/null || echo '?')"

    # 版本检查
    if [[ -n "$RUST_VER" ]]; then
        if [[ "$(echo -e "$RUST_VER\n$MIN_RUST_VERSION" | sort -V | head -1)" != "$MIN_RUST_VERSION" ]]; then
            warn "Rust $RUST_VER 低于推荐版本 $MIN_RUST_VERSION，建议升级"
            warn "  运行: rustup update stable"
        fi
    fi
}

# 3b. protoc (用于 gRPC 编译)
check_protoc() {
    if ! command -v protoc &>/dev/null; then
        warn "未安装 protoc（protobuf 编译器）"
        warn "  某些 gRPC 功能在缺少 protoc 时会跳过编译，不影响核心功能"
        case "$OS" in
            termux)           warn "  安装: pkg install protobuf" ;;
            termux-proot|debian|ubuntu) warn "  安装: apt install protobuf-compiler" ;;
            macos)            warn "  安装: brew install protobuf" ;;
            rhel|fedora)      warn "  安装: dnf install protobuf-compiler" ;;
            alpine)           warn "  安装: apk add protobuf" ;;
        esac
    else
        ok "protoc: $(protoc --version 2>/dev/null || echo '?')"
    fi
}

# 3c. C 编译工具链 (Rust 链接时需要)
check_cc() {
    if ! command -v cc &>/dev/null && ! command -v gcc &>/dev/null && ! command -v clang &>/dev/null; then
        warn "未找到 C 编译器（极少情况下 Rust 链接时需要）"
        case "$OS" in
            termux)           warn "  安装: pkg install clang" ;;
            termux-proot|debian|ubuntu) warn "  安装: apt install build-essential" ;;
            macos)            warn "  安装: xcode-select --install" ;;
            rhel|fedora)      warn "  安装: dnf groupinstall 'Development Tools'" ;;
            alpine)           warn "  安装: apk add build-base" ;;
        esac
    else
        local cc
        cc=$(command -v gcc || command -v clang || command -v cc || echo "cc")
        ok "C 编译器: $($cc --version 2>/dev/null | head -1)"
    fi
}

# 3d. pkg-config + openssl (常见链接依赖)
check_pkgconfig() {
    if ! command -v pkg-config &>/dev/null; then
        warn "未安装 pkg-config（某些依赖需要）"
        case "$OS" in
            termux)           warn "  安装: pkg install pkg-config" ;;
            termux-proot|debian|ubuntu) warn "  安装: apt install pkg-config" ;;
            macos)            warn "  已内置（可通过 brew install pkg-config 更新）" ;;
            rhel|fedora)      warn "  安装: dnf install pkgconfig" ;;
            alpine)           warn "  安装: apk add pkgconfig" ;;
        esac
    else
        ok "pkg-config: $(pkg-config --version 2>/dev/null || echo '?')"
    fi

    # openssl (按需提示)
    if ! pkg-config --exists openssl 2>/dev/null && [[ "$OS" != "macos" ]]; then
        if [[ "$OS" == "termux" ]]; then
            # Termux 通常有 openssl
            :
        else
            warn "未检测到 openssl 开发库（需要时安装）"
            case "$OS" in
                termux-proot|debian|ubuntu) warn "  安装: apt install libssl-dev" ;;
                rhel|fedora)  warn "  安装: dnf install openssl-devel" ;;
                alpine)       warn "  安装: apk add openssl-dev" ;;
            esac
        fi
    fi
}

# 3e. WASM 目标 (WebUI 构建需要)
check_wasm() {
    local wasm_target
    wasm_target=$(rustup target list --installed 2>/dev/null | grep wasm32-unknown-unknown || true)
    if [[ -z "$wasm_target" ]]; then
        warn "未安装 WASM 编译目标，WebUI 需要额外安装:"
        warn "  rustup target add wasm32-unknown-unknown"
    else
        ok "WASM 目标: 已安装"
    fi

    if command -v trunk &>/dev/null; then
        ok "trunk: $(trunk --version 2>/dev/null || echo '?')"
    else
        warn "未安装 trunk（WebUI 构建需要）"
        warn "  安装: cargo install trunk"
    fi
}

# ── 执行所有检查 ──
check_rust
check_protoc
check_cc
check_pkgconfig
check_wasm

echo ""
if $PASS_ALL; then
    ok "所有依赖已满足 🎉"
else
    warn "部分依赖缺失，但核心功能仍可运行（详见上方提示）"
fi

# ── --check-env 模式：仅检查不启动 ──
if $CHECK_ENV; then
    echo ""
    info "环境检查完成。运行 ./start.sh --help 查看启动选项"
    exit 0
fi

# --- doctor: 全面诊断 ---
if $DOCTOR; then
    header "🔍 系统诊断报告"
    
    # 系统信息
    echo ""
    echo -e "${BOLD}系统信息${NC}"
    echo -e "  操作系统:  $(uname -s) $(uname -r)"
    echo -e "  架构:      $(uname -m)"
    echo -e "  主机名:    $(hostname 2>/dev/null || echo 'N/A')"
    if command -v free &>/dev/null; then
        MEM_TOTAL=$(free -h | awk '/Mem:/ {print $2}')
        MEM_AVAIL=$(free -h | awk '/Mem:/ {print $7}')
        echo -e "  内存:      ${MEM_TOTAL} (可用 ${MEM_AVAIL})"
    fi
    if command -v df &>/dev/null; then
        DISK=$(df -h . | awk 'NR==2 {print $4}')
        echo -e "  磁盘剩余:  ${DISK}"
    fi
    
    # Rust 工具链
    echo ""
    echo -e "${BOLD}Rust 工具链${NC}"
    if command -v rustc &>/dev/null; then
        echo -e "  ${GREEN}✔${NC} rustc:  $(rustc --version 2>/dev/null)"
        echo -e "  ${GREEN}✔${NC} cargo:  $(cargo --version 2>/dev/null)"
        # Check for clippy
        if cargo clippy --version &>/dev/null; then
            echo -e "  ${GREEN}✔${NC} clippy: $(cargo clippy --version 2>/dev/null)"
        else
            echo -e "  ${YELLOW}⚠${NC} clippy: 未安装 (运行: rustup component add clippy)"
        fi
        # Check for rustfmt
        if rustfmt --version &>/dev/null; then
            echo -e "  ${GREEN}✔${NC} rustfmt: $(rustfmt --version 2>/dev/null)"
        else
            echo -e "  ${YELLOW}⚠${NC} rustfmt: 未安装 (运行: rustup component add rustfmt)"
        fi
        # WASM target
        if rustup target list --installed 2>/dev/null | grep -q wasm32; then
            echo -e "  ${GREEN}✔${NC} WASM:    已安装"
        else
            echo -e "  ${YELLOW}⚠${NC} WASM:    未安装 (运行: rustup target add wasm32-unknown-unknown)"
        fi
    else
        echo -e "  ${RED}✘${NC} Rust:    未安装"
    fi
    
    # 系统库
    echo ""
    echo -e "${BOLD}系统库${NC}"
    for cmd in protoc cc gcc clang pkg-config make cmake; do
        if command -v "$cmd" &>/dev/null; then
            echo -e "  ${GREEN}✔${NC} $cmd:    $($cmd --version 2>/dev/null | head -1)"
        else
            echo -e "  ${YELLOW}⚠${NC} $cmd:    未安装"
        fi
    done
    
    # API Key 检查
    echo ""
    echo -e "${BOLD}API Key 状态${NC}"
    source_env() {
        if [[ -f .env ]]; then
            set -a; source .env; set +a 2>/dev/null || true
        fi
    }
    source_env
    for key_var in OPENAI_API_KEY DEEPSEEK_API_KEY QWEN_API_KEY ANTHROPIC_API_KEY GROQ_API_KEY; do
        val=$(eval echo \${$key_var:-})
        if [[ -n "$val" ]]; then
            masked="${val:0:8}...${val: -4}"
            echo -e "  ${GREEN}✔${NC} $key_var: $masked"
        else
            echo -e "  ${YELLOW}⚠${NC} $key_var: 未配置"
        fi
    done
    
    # 通道配置
    echo ""
    echo -e "${BOLD}消息通道${NC}"
    for ch_var in LINGSHU_TELEGRAM_BOT_TOKEN LINGSHU_FEISHU_APP_ID LINGSHU_QQ_APP_ID; do
        val=$(eval echo \${$ch_var:-})
        if [[ -n "$val" ]]; then
            echo -e "  ${GREEN}✔${NC} $ch_var: 已配置"
        else
            echo -e "  ${YELLOW}⚠${NC} $ch_var: 未配置 (可选)"
        fi
    done
    
    # 网络连通性
    echo ""
    echo -e "${BOLD}网络检查${NC}"
    if $CHINA; then
        echo -e "  ${GREEN}✔${NC} 国内网络模式启用 — 跳过境外站点检测"
        for target in "https://api.deepseek.com" "https://mirrors.ustc.edu.cn" "https://www.baidu.com"; do
            if curl -sf --max-time 5 "$target" &>/dev/null; then
                echo -e "  ${GREEN}✔${NC} $(echo $target | sed 's|https://||'): 可达"
            else
                echo -e "  ${YELLOW}⚠${NC} $(echo $target | sed 's|https://||'): 不可达 (可能需要代理)"
            fi
        done
        echo ""
        echo -e "  ${GREEN}✔${NC} DeepSeek / 千问 等国内 API 一般可直接访问"
        echo -e "  ${YELLOW}⚠${NC} 如需访问境外 API (OpenAI 等)，请配置代理"
    else
        for target in "https://api.openai.com" "https://api.deepseek.com" "https://api.github.com"; do
            if curl -sf --max-time 5 "$target" &>/dev/null; then
                echo -e "  ${GREEN}✔${NC} $(echo $target | sed 's|https://||'): 可达"
            else
                echo -e "  ${RED}✘${NC} $(echo $target | sed 's|https://||'): 不可达 (检查网络/代理)"
            fi
        done
    fi
    
    # 目录权限
    echo ""
    echo -e "${BOLD}目录权限${NC}"
    for dir in "." "$HOME/.local/share/lingshu" "$HOME/.config/lingshu"; do
        if [[ -w "$dir" ]] || mkdir -p "$dir" 2>/dev/null; then
            echo -e "  ${GREEN}✔${NC} $dir: 可写"
        else
            echo -e "  ${RED}✘${NC} $dir: 不可写"
        fi
    done
    
    echo ""
    echo -e "${BOLD}━━━ 诊断完成 ${NC}"
    exit 0
fi

# --- update: 拉取最新代码并重新编译 ---
if $UPDATE; then
    header "🔄 更新 Lingshu"
    
    # 检查 git 仓库
    if [[ ! -d .git ]]; then
        err "不是 git 仓库，无法自动更新"
        err "请手动下载最新版本: https://github.com/malaxiya2019/ling-shu/releases"
        exit 1
    fi
    
    info "拉取最新代码..."
    if ! git pull --rebase 2>&1; then
        err "拉取失败，请手动处理冲突后重新运行"
        exit 1
    fi
    ok "代码已更新到最新"
    
    NEWEST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "unknown")
    info "最新标签: $NEWEST_TAG"
    
    info "重新编译..."
    START_TIME=$(date +%s)
    cargo build --release -p lingshu 2>&1 | tail -3
    END_TIME=$(date +%s)
    BUILD_DURATION=$((END_TIME - START_TIME))
    ok "编译完成 (耗时 ${BUILD_DURATION} 秒)"
    
    # 安装到 ~/.cargo/bin （可选，方便直接调用）
    if [[ -d "$HOME/.cargo/bin" ]]; then
        info "安装到 \$HOME/.cargo/bin..."
        cp target/release/lingshu "$HOME/.cargo/bin/lingshu" 2>/dev/null &&             ok "已安装: \$HOME/.cargo/bin/lingshu" ||             warn "安装失败，请手动复制: cp target/release/lingshu \$HOME/.cargo/bin/"
    fi
    
    # 如果有 Node.js，也更新 openclaw-bridge
    if command -v node &>/dev/null && [[ -d "examples/openclaw-bridge" ]]; then
        info "更新 OpenClaw Bridge..."
        (cd examples/openclaw-bridge && npm install --silent && npm run build) 2>/dev/null &&             ok "OpenClaw Bridge 已更新" ||             warn "OpenClaw Bridge 更新跳过"
    fi
    
    # 显示版本变化
    OLD_VERSION=$(git describe --tags --abbrev=0 HEAD~1 2>/dev/null || echo "无")
    NEW_VERSION="${NEWEST_TAG}"
    if [[ "$OLD_VERSION" != "$NEW_VERSION" && "$OLD_VERSION" != "无" ]]; then
        echo ""
        echo -e "${BOLD}版本变化:${NC}"
        git log --oneline "${OLD_VERSION}..${NEW_VERSION}" 2>/dev/null | head -20 | while read -r line; do
            echo "  • $line"
        done
        COMMIT_COUNT=$(git log --oneline "${OLD_VERSION}..${NEW_VERSION}" 2>/dev/null | wc -l)
        if [[ "$COMMIT_COUNT" -gt 20 ]]; then
            echo "  ... 还有 $((COMMIT_COUNT - 20)) 个提交未显示"
        fi
    fi
    
    echo ""
    echo -e "${GREEN}✅ 更新完成！${NC}"
    echo "  运行 ./start.sh --quick 启动新版本"
    echo "  运行 lingshu (PATH 中) 直接启动已安装的二进制"
    exit 0
fi

# ── 4. 检查 / 创建 .env 文件 ──
if [[ ! -f .env ]] && ! $QUICK; then
    header "📝 首次配置向导"

    if [[ -f .env.example ]]; then
        cp .env.example .env
        info "已从 .env.example 创建 .env 文件"
    fi

    echo ""
    echo "请选择 LLM 提供商 (输入编号):"
    echo "  1) OpenAI — 默认 (需要 OPENAI_API_KEY)"
    echo "  2) DeepSeek — 国产推荐，性价比极高"
    echo "  3) 阿里千问 Qwen — 国产"
    echo "  4) Anthropic Claude"
    echo "  5) Mock — 无 API Key，仅测试框架功能"
    echo "  6) 跳过配置，稍后手动编辑"
    read -r -p "选择 [1]: " provider_choice
    provider_choice="${provider_choice:-1}"

    case "$provider_choice" in
        1|"")
            sed -i 's/^LLM_PROVIDER=.*/LLM_PROVIDER=openai/' .env
            if grep -q 'OPENAI_API_KEY=sk-' .env 2>/dev/null && \
               ! grep -q 'OPENAI_API_KEY=sk-xxxxxxxxxxxx' .env 2>/dev/null; then
                ok "OPENAI_API_KEY 已配置"
            else
                read -r -p "请输入 OpenAI API Key (sk-...): " api_key
                if [[ -n "$api_key" ]]; then
                    # 兼容 macOS sed
                    if [[ "$(uname -s)" == "Darwin" ]]; then
                        sed -i '' "s|^OPENAI_API_KEY=sk-xxxxxxxxxxxx|OPENAI_API_KEY=$api_key|" .env
                    else
                        sed -i "s|^OPENAI_API_KEY=sk-xxxxxxxxxxxx|OPENAI_API_KEY=$api_key|" .env
                    fi
                    ok "OpenAI API Key 已设置"
                else
                    warn "未设置 API Key，编辑 .env 文件补充即可"
                fi
            fi
            ;;
        2)
            sed -i 's/^LLM_PROVIDER=.*/LLM_PROVIDER=deepseek/' .env
            read -r -p "请输入 DeepSeek API Key (sk-...): " api_key
            if [[ -n "$api_key" ]]; then
                if [[ "$(uname -s)" == "Darwin" ]]; then
                    sed -i '' "s|^DEEPSEEK_API_KEY=sk-xxxxxxxxxxxx|DEEPSEEK_API_KEY=$api_key|" .env
                else
                    sed -i "s|^DEEPSEEK_API_KEY=sk-xxxxxxxxxxxx|DEEPSEEK_API_KEY=$api_key|" .env
                fi
                ok "DeepSeek API Key 已设置"
            fi
            ;;
        3)
            sed -i 's/^LLM_PROVIDER=.*/LLM_PROVIDER=qwen/' .env
            read -r -p "请输入千问 API Key (sk-...): " api_key
            if [[ -n "$api_key" ]]; then
                if [[ "$(uname -s)" == "Darwin" ]]; then
                    sed -i '' "s|^QWEN_API_KEY=sk-xxxxxxxxxxxx|QWEN_API_KEY=$api_key|" .env
                else
                    sed -i "s|^QWEN_API_KEY=sk-xxxxxxxxxxxx|QWEN_API_KEY=$api_key|" .env
                fi
                ok "千问 API Key 已设置"
            fi
            ;;
        4)
            sed -i 's/^LLM_PROVIDER=.*/LLM_PROVIDER=anthropic/' .env
            read -r -p "请输入 Anthropic API Key (sk-ant-...): " api_key
            if [[ -n "$api_key" ]]; then
                if [[ "$(uname -s)" == "Darwin" ]]; then
                    sed -i '' "s|^ANTHROPIC_API_KEY=sk-ant-xxxxxxxxxxxx|ANTHROPIC_API_KEY=$api_key|" .env
                else
                    sed -i "s|^ANTHROPIC_API_KEY=sk-ant-xxxxxxxxxxxx|ANTHROPIC_API_KEY=$api_key|" .env
                fi
                ok "Anthropic API Key 已设置"
            fi
            ;;
        5)
            sed -i 's/^LLM_PROVIDER=.*/LLM_PROVIDER=mock/' .env
            warn "Mock 模式：LLM 返回模拟响应，仅用于测试框架基本流程"
            ;;
        6)
            info "跳过配置，稍后手动编辑 .env 文件即可"
            ;;
    esac
    echo ""
    echo -e "${GREEN}✅ 配置完成！${NC}"
    echo "  编辑 .env 文件可随时修改配置"
elif $QUICK && [[ ! -f .env ]]; then
    warn "未找到 .env 文件，使用默认配置（无 API Key，LLM 将不可用）"
    warn "  创建 .env 文件: cp .env.example .env"
fi

# ── 5. 编译 ──
header "🔨 编译项目"

# 检测可用内存，决定并行度（避免 OOM）
if command -v free &>/dev/null; then
    MEM_MB=$(free -m | awk '/Mem:/ {print $7}')
    if [[ -n "$MEM_MB" && "$MEM_MB" -lt 2048 ]]; then
        info "可用内存 ${MEM_MB}MB，使用低并行度编译 (避免 OOM)"
        BUILD_JOBS="--jobs 2"
    else
        BUILD_JOBS=""
    fi
elif command -v vm_stat &>/dev/null; then
    # macOS
    info "macOS 自动管理编译并行度"
    BUILD_JOBS=""
else
    BUILD_JOBS=""
fi

# 可选构建 WebUI
if [[ -d "webui" ]] && command -v trunk &>/dev/null; then
    if rustup target list --installed 2>/dev/null | grep -q "wasm32-unknown-unknown"; then
        info "🌐 检测到 WebUI 构建环境，编译 WASM..."
        (cd webui && trunk build --release 2>&1 | tail -3) &&             ok "WebUI 构建完成" ||             warn "WebUI 编译失败（不影响核心功能）"
    fi
fi

info "环境: ${ENV} | 监听: ${ADDR} | REPL: ${REPL}"
info "编译中，首次编译需要下载依赖，耗时 5-15 分钟..."
info "后续编译仅需 30-60 秒"

START_TIME=$(date +%s)
cargo build --release $BUILD_JOBS -p lingshu 2>&1 | tail -5
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))
if [[ $DURATION -gt 120 ]]; then
    ok "编译完成 (耗时 ${DURATION} 秒，首次编译正常)"
else
    ok "编译完成 (耗时 ${DURATION} 秒，缓存命中)"
fi

# ── 7. 启动 ──
header "🌐 启动服务"


# ── 6a. OpenClaw Bridge ──
if $WITH_OPENCLAW; then
    if command -v node &>/dev/null && command -v npm &>/dev/null; then
        OPENCLAW_DIR="examples/openclaw-bridge"
        if [[ -d "$OPENCLAW_DIR" ]]; then
            info "🔌 构建 OpenClaw Bridge..."
            (cd "$OPENCLAW_DIR" && npm install --silent && npm run build) || warn "OpenClaw Bridge 构建失败，跳过"
            ok "OpenClaw Bridge 构建完成"
            # 设置 HTTP 端口，供 lingshu MCP 客户端连接
            export OPENCLAW_HTTP_PORT=18931
            info "OpenClaw Bridge HTTP 端口: $OPENCLAW_HTTP_PORT"
            # 启动 openclaw-bridge (后台进程)
            (cd "$OPENCLAW_DIR" && HTTP_PORT=$OPENCLAW_HTTP_PORT node dist/index.js &)
            sleep 1
            ok "OpenClaw Bridge 已启动 (PID: $!)"
        else
            warn "OpenClaw Bridge 目录不存在: $OPENCLAW_DIR"
        fi
    else
        warn "Node.js 未安装，跳过 OpenClaw Bridge"
    fi
fi

EXTRA_ARGS=""
$REPL && EXTRA_ARGS="$EXTRA_ARGS --repl"
EXTRA_ARGS="$EXTRA_ARGS -e $ENV"
EXTRA_ARGS="$EXTRA_ARGS --addr $ADDR"

echo ""
echo -e "${BOLD}   Lingshu ${GREEN}v3.4${NC}${BOLD} Agent System${NC}"
echo -e "   ${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "   环境:    ${YELLOW}${ENV}${NC}"
echo -e "   监听:    ${YELLOW}${ADDR}${NC}"
echo -e "   模式:    $($REPL && echo '${YELLOW}REPL${NC}' || echo '${YELLOW}HTTP API Server${NC}')"
echo -e "   数据:    ${YELLOW}\$HOME/.local/share/lingshu${NC}"
echo -e "   ${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
if ! $REPL; then
    echo "   📋 API 文档:  http://${ADDR}/docs"
    echo "   📊 管理面板:  http://${ADDR}/admin"
    echo "   🖥  WebUI:     http://${ADDR}/webui"
    echo "   💬 Chat API:  POST http://${ADDR}/v1/chat/completions"
    echo ""
    echo "   测试: curl http://${ADDR}/health"
fi
echo ""

exec ./target/release/lingshu $EXTRA_ARGS
