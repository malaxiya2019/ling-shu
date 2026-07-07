#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────────
# Lingshu 一键安装脚本
# 适用:
#   - Termux (Android)
#   - Ubuntu (原生 / Termux proot)
# 用法:
#   bash <(curl -fsSL https://raw.githubusercontent.com/malaxiya2019/ling-shu/main/scripts/install.sh)
#   或: sudo bash install.sh
# ─────────────────────────────────────────────────────────

REPO_URL="https://github.com/malaxiya2019/ling-shu.git"
INSTALL_DIR="${LINGSHU_HOME:-$HOME/ling-shu}"
BIN_DIR="${LINGSHU_BIN_DIR:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/lingshu"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/lingshu"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERR]${NC}   $*"; }

# ── 1. 检测系统 ──
# 返回: termux | termux-proot | ubuntu | unknown
detect_os() {
    local in_termux=false
    local is_ubuntu=false

    [[ -d /data/data/com.termux/files/usr ]] && in_termux=true
    grep -qi ubuntu /etc/os-release 2>/dev/null && is_ubuntu=true

    if $in_termux && $is_ubuntu; then
        echo "termux-proot"          # Ubuntu in Termux proot
    elif $in_termux; then
        echo "termux"                # Native Termux
    elif $is_ubuntu; then
        echo "ubuntu"                # Native Ubuntu
    else
        echo "unknown"
    fi
}

# ── 2. 安装系统依赖 + Rust ──
install_deps() {
    info "安装系统依赖..."
    case "$(detect_os)" in
        termux)
            pkg update -y
            pkg install -y rust binutils clang llvm lld libsqlite openssl pkg-config make git curl
            ;;
        termux-proot|ubuntu)
            apt update -y
            apt install -y build-essential pkg-config libssl-dev libsqlite3-dev llvm clang lld cmake make git curl
            if ! command -v rustc &>/dev/null; then
                info "通过 rustup 安装 Rust..."
                curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
                . "$HOME/.cargo/env"
            fi
            ;;
        *)
            err "不支持的系统，仅支持 Termux / Ubuntu"
            exit 1
            ;;
    esac
    ok "$(detect_os) 依赖已安装"
    info "Rust: $(rustc --version)  |  Cargo: $(cargo --version)"
}

# ── 3. 克隆代码 ──
clone_repo() {
    if [[ -d "$INSTALL_DIR/.git" ]]; then
        info "项目已存在，拉取最新代码..."
        cd "$INSTALL_DIR" && git pull --rebase
        ok "代码已更新"
    else
        info "克隆项目到 $INSTALL_DIR ..."
        git clone --depth 1 "$REPO_URL" "$INSTALL_DIR"
        ok "代码已克隆"
    fi
}

# ── 4. 编译 ──
build_project() {
    info "编译 Lingshu（Release 模式，耗时较长）..."
    cd "$INSTALL_DIR"
    local jobs=""
    if command -v nproc &>/dev/null; then
        local cpus
        cpus=$(nproc)
        jobs="-j $((cpus > 4 ? 4 : cpus))"
    fi
    CARGO_BUILD_JOBS="${jobs##-j }" cargo build --release $jobs -p lingshu
    ok "编译完成"
}

# ── 5. 安装 ──
install_binary() {
    mkdir -p "$BIN_DIR" "$DATA_DIR" "$CONFIG_DIR"
    cp "$INSTALL_DIR/target/release/lingshu" "$BIN_DIR/lingshu"
    ok "二进制已安装: $BIN_DIR/lingshu ($(ls -lh "$BIN_DIR/lingshu" | awk '{print $5}'))"

    if [[ ! -f "$CONFIG_DIR/.env" ]]; then
        cp "$INSTALL_DIR/.env.example" "$CONFIG_DIR/.env"
        sed -i "s|^LS_STORAGE_DIR=.*|LS_STORAGE_DIR=$DATA_DIR/storage|" "$CONFIG_DIR/.env"
        sed -i "s|^LS_DATABASE_URL=.*|LS_DATABASE_URL=$DATA_DIR/lingshu.db|" "$CONFIG_DIR/.env"
        warn "请编辑 $CONFIG_DIR/.env 配置 API Key（当前为 mock 模式）"
    fi

    local rc="$HOME/.bashrc"
    if ! grep -q "LINGSHU_HOME" "$rc" 2>/dev/null; then
        cat >> "$rc" <<-EOF

# Lingshu
export LINGSHU_HOME="$INSTALL_DIR"
export PATH="\$PATH:$BIN_DIR"
export LS_ENV=dev
EOF
        ok "已添加 PATH 到 $rc"
        info "执行 source $rc 或开新终端生效"
    fi
}

# ── 6. 验证 ──
verify() {
    echo ""
    info "验证安装..."
    if "$BIN_DIR/lingshu" --help &>/dev/null; then
        ok "安装成功！"
        "$BIN_DIR/lingshu" --help 2>&1 | head -3
    else
        warn "直接运行失败，尝试 cargo run..."
        cd "$INSTALL_DIR" && cargo run -p lingshu -- --help 2>&1 | head -3 || true
    fi
}

# ── 7. systemd 服务（仅原生 Ubuntu，不含 proot） ──
setup_service() {
    [[ "$(detect_os)" != "ubuntu" ]] && return
    [[ -f /etc/systemd/system/lingshu.service ]] && return
    info "设置 systemd 服务..."
    cat > /etc/systemd/system/lingshu.service <<-EOF
[Unit]
Description=Lingshu Agent Service
After=network.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$INSTALL_DIR
Environment=LS_ENV=prod
EnvironmentFile=$CONFIG_DIR/.env
ExecStart=$BIN_DIR/lingshu --serve --addr 0.0.0.0:8080
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
    systemctl daemon-reload
    ok "systemd 服务已创建"
    echo "   systemctl enable lingshu"
    echo "   systemctl start lingshu"
}

# ── 主流程 ──
main() {
    local os
    os=$(detect_os)

    echo ""
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}   Lingshu 一键安装${NC}"
    echo -e "${CYAN}   系统: ${os}${NC}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""

    cd /tmp

    # 非 Termux 环境需要 root 来安装系统包
    if [[ "$os" != "termux" && "$EUID" != "0" ]]; then
        err "请用 sudo 执行: sudo bash install.sh"
        exit 1
    fi

    install_deps
    clone_repo
    build_project
    install_binary
    verify
    setup_service

    echo ""
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}  Lingshu 安装成功！${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "  二进制:  $BIN_DIR/lingshu"
    echo "  源码:    $INSTALL_DIR"
    echo "  数据:    $DATA_DIR"
    echo "  配置:    $CONFIG_DIR/.env"
    echo ""
    echo -e "  ${CYAN}lingshu${NC}              # REPL 交互模式"
    echo -e "  ${CYAN}lingshu --serve${NC}      # HTTP 服务器模式"
    echo ""
    echo -e "  首次使用: 编辑 ${YELLOW}$CONFIG_DIR/.env${NC} 填入 API Key"
    echo "  当前为 mock 模式，无需 Key 可直接运行"
    echo ""
}

main "$@"
