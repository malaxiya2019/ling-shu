#!/usr/bin/env bash
# =============================================================================
#  LingShu Benchmark 基线测量脚本 v4.2.7 LTS
# =============================================================================
# 测量以下基准:
#   1. 启动时间 (cold start -> ready)
#   2. 空闲内存占用
#   3. 负载中内存占用
#   4. 吞吐量 (RPS)
#   5. 延迟分布 (P50/P90/P95/P99)
#
# 用法:
#   ./scripts/benchmark_baseline.sh                      # 完整基准
#   ./scripts/benchmark_baseline.sh --startup             # 仅启动时间
#   ./scripts/benchmark_baseline.sh --memory              # 仅内存
#   ./scripts/benchmark_baseline.sh --throughput           # 仅吞吐量
#   ./scripts/benchmark_baseline.sh --all                  # 全部 (默认)
#
# 输出:
#   benchmark_results_<timestamp>/ 目录，含 JSON 报告
# =============================================================================

set -euo pipefail

# ── 配置 ──
BASE_URL="${1:-http://localhost:8080}"
MODE="${2:-all}"
BENCH_DIR="benchmark_results_$(date +%Y%m%d_%H%M%S)"
LINGSHU_BIN="./target/debug/lingshu"

# 解析参数
for arg in "$@"; do
    case "$arg" in
        --startup)    MODE="startup";;
        --memory)     MODE="memory";;
        --throughput) MODE="throughput";;
        --latency)    MODE="latency";;
        --all)       MODE="all";;
        --help|-h)
            echo "用法: $0 [url] [mode]"
            echo "     mode: --startup | --memory | --throughput | --latency | --all"
            exit 0;;
    esac
done

# ── 颜色 ──
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
err()   { echo -e "${RED}[ERR]${NC} $*" >&2; }
header(){ echo -e "\n${BOLD}━━━ $* ━━━${NC}"; }

mkdir -p "$BENCH_DIR"

# ============================================================
# 1. 启动时间测量
# ============================================================
measure_startup() {
    header "1/4 启动时间测量"

    local binary="$LINGSHU_BIN"
    if [[ ! -f "$binary" ]]; then
        # 尝试找 release binary
        binary="./target/release/lingshu"
        if [[ ! -f "$binary" ]]; then
            warn "未找到 lingshu binary (${LINGSHU_BIN})，尝试 cargo build..."
            cargo build 2>/dev/null || {
                err "无法编译 lingshu，跳过启动时间测试"
                return
            }
            binary="$LINGSHU_BIN"
        fi
    fi

    local warmup_count=1
    local measure_count=3
    local startup_times=()

    # 确保没有旧进程
    pkill -f "target/.*/lingshu" 2>/dev/null || true
    sleep 2

    echo -e "  测量次数: ${measure_count}"
    echo ""

    for ((i=1; i<=measure_count; i++)); do
        echo -e "  运行 ${i}/${measure_count}..."

        # 后台启动
        local start_ts=$(date +%s%N)
        "$binary" &
        local pid=$!

        # 等待健康检查通过
        local timeout=60
        local ready=false
        for ((t=0; t<timeout; t++)); do
            sleep 1
            local code
            code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 2 "${BASE_URL}/health" 2>/dev/null || echo "")
            if [[ "$code" == "200" ]]; then
                ready=true
                break
            fi
        done

        local end_ts=$(date +%s%N)
        local elapsed_ms=$(( (end_ts - start_ts) / 1000000 ))

        if $ready; then
            startup_times+=("$elapsed_ms")
            echo -e "    ${GREEN}✔${NC} 就绪: ${elapsed_ms}ms (PID: ${pid})"

            # 保持运行一会儿供后续测试
            if [[ $i -lt $measure_count ]]; then
                kill "$pid" 2>/dev/null || true
                sleep 3
            fi
        else
            echo -e "    ${RED}✘${NC} 超时 (${timeout}s)，未就绪"
            kill "$pid" 2>/dev/null || true
        fi
    done

    if [[ ${#startup_times[@]} -gt 0 ]]; then
        local total=0 min=${startup_times[0]} max=0
        for t in "${startup_times[@]}"; do
            total=$((total + t))
            [[ $t -lt $min ]] && min=$t
            [[ $t -gt $max ]] && max=$t
        done
        local avg=$((total / ${#startup_times[@]}))

        echo -e "\n  结果:"
        echo -e "    Min:  ${BLUE}${min}ms${NC}"
        echo -e "    Avg:  ${BLUE}${avg}ms${NC}"
        echo -e "    Max:  ${BLUE}${max}ms${NC}"

        # 保存结果
        cat > "${BENCH_DIR}/startup.json" <<JSON
{
  "metric": "startup_time_ms",
  "values": [$(IFS=,; echo "${startup_times[*]}")],
  "min": $min,
  "avg": $avg,
  "max": $max,
  "samples": ${#startup_times[@]},
  "timestamp": "$(date -Iseconds)"
}
JSON
        info "结果保存至 ${BENCH_DIR}/startup.json"
    fi
}

# ============================================================
# 2. 内存占用测量
# ============================================================
measure_memory() {
    header "2/4 内存占用测量"

    local pid
    pid=$(pgrep -f "target/.*/lingshu" | head -1 2>/dev/null || echo "")

    if [[ -z "$pid" ]]; then
        warn "lingshu 未运行，启动中..."
        if [[ -f "$LINGSHU_BIN" ]]; then
            "$LINGSHU_BIN" &
            sleep 10
            pid=$(pgrep -f "target/.*/lingshu" | head -1 2>/dev/null || echo "")
        else
            err "无法启动 lingshu"
            return
        fi
    fi

    echo -e "  监控进程: lingshu (PID: ${pid})"

    # 空闲内存
    echo -e "\n  ${BOLD}空闲状态 (idel)${NC}"
    local idle_samples=5
    local idle_values=()
    for ((i=1; i<=idle_samples; i++)); do
        local rss_kb
        if [[ -f "/proc/${pid}/status" ]]; then
            rss_kb=$(grep -i "VmRSS" "/proc/${pid}/status" | awk '{print $2}')
        else
            rss_kb=$(ps -o rss= -p "$pid" 2>/dev/null | tr -d ' ')
        fi
        idle_values+=("${rss_kb:-0}")
        local rss_mb=$(echo "scale=1; ${rss_kb:-0}/1024" | bc)
        echo -e "    采样 ${i}: ${rss_mb} MB (RSS)"
        sleep 1
    done

    # 负载内存 (发并发请求)
    echo -e "\n  ${BOLD}负载状态 (under load)${NC}"
    # 先发一些并发请求
    for ((i=1; i<=20; i++)); do
        curl -s -o /dev/null "${BASE_URL}/health" &
    done
    wait 2>/dev/null || true
    sleep 2

    local load_samples=5
    local load_values=()
    for ((i=1; i<=load_samples; i++)); do
        local rss_kb
        if [[ -f "/proc/${pid}/status" ]]; then
            rss_kb=$(grep -i "VmRSS" "/proc/${pid}/status" | awk '{print $2}')
        else
            rss_kb=$(ps -o rss= -p "$pid" 2>/dev/null | tr -d ' ')
        fi
        load_values+=("${rss_kb:-0}")
        local rss_mb=$(echo "scale=1; ${rss_kb:-0}/1024" | bc)
        echo -e "    采样 ${i}: ${rss_mb} MB (RSS)"
        # 持续发请求
        for ((j=1; j<=5; j++)); do
            curl -s -o /dev/null "${BASE_URL}/health" &
        done
        sleep 1
    done
    wait 2>/dev/null || true

    # 计算统计
    compute_stats() {
        local name="$1"
        shift
        local values=("$@")
        local total=0 min=${values[0]} max=0
        for v in "${values[@]}"; do
            total=$((total + v))
            [[ $v -lt $min ]] && min=$v
            [[ $v -gt $max ]] && max=$v
        done
        local avg=$((total / ${#values[@]}))
        echo "$min $avg $max"
    }

    local idle_stats=($(compute_stats "idle" "${idle_values[@]}"))
    local load_stats=($(compute_stats "load" "${load_values[@]}"))

    echo -e "\n  结果:"
    echo -e "    空闲 RSS: Min=${idle_stats[0]}KB Avg=${idle_stats[1]}KB Max=${idle_stats[2]}KB"
    echo -e "          = Min=$(echo "scale=1; ${idle_stats[0]}/1024" | bc)MB Avg=$(echo "scale=1; ${idle_stats[1]}/1024" | bc)MB Max=$(echo "scale=1; ${idle_stats[2]}/1024" | bc)MB"
    echo -e "    负载 RSS: Min=${load_stats[0]}KB Avg=${load_stats[1]}KB Max=${load_stats[2]}KB"
    echo -e "          = Min=$(echo "scale=1; ${load_stats[0]}/1024" | bc)MB Avg=$(echo "scale=1; ${load_stats[1]}/1024" | bc)MB Max=$(echo "scale=1; ${load_stats[2]}/1024" | bc)MB"

    local diff=$((load_stats[1] - idle_stats[1]))
    echo -e "    负载增量: ${diff}KB ($(echo "scale=1; ${diff}/1024" | bc)MB)"

    cat > "${BENCH_DIR}/memory.json" <<JSON
{
  "metric": "memory_usage_kb",
  "idle": {
    "values": [$(IFS=,; echo "${idle_values[*]}")],
    "min": ${idle_stats[0]},
    "avg": ${idle_stats[1]},
    "max": ${idle_stats[2]}
  },
  "load": {
    "values": [$(IFS=,; echo "${load_values[*]}")],
    "min": ${load_stats[0]},
    "avg": ${load_stats[1]},
    "max": ${load_stats[2]}
  },
  "delta_kb": $diff,
  "timestamp": "$(date -Iseconds)"
}
JSON
    info "结果保存至 ${BENCH_DIR}/memory.json"
}

# ============================================================
# 3. 吞吐量 (RPS) 测量
# ============================================================
measure_throughput() {
    header "3/4 吞吐量测量 (RPS)"

    # 检测是否有 wrk
    local has_wrk=false
    command -v wrk &>/dev/null && has_wrk=true

    if $has_wrk; then
        echo -e "  使用 wrk (更精确)"
        local results=()

        # 不同并发级别
        for concurrency in 1 5 10 25 50; do
            echo -e "\n  ${CYAN}并发: ${concurrency}${NC}"
            local output
            output=$(wrk -t2 -c"${concurrency}" -d10s --latency "${BASE_URL}/health" 2>/dev/null) || {
                warn "wrk 失败"
                continue
            }
            local rps=$(echo "$output" | grep "Requests/sec" | awk '{print $2}')
            local latency_avg=$(echo "$output" | grep "Latency" | head -1 | awk '{print $2}')
            local latency_stddev=$(echo "$output" | grep "Latency" | head -1 | awk '{print $3}')
            echo -e "    RPS: ${GREEN}${rps}${NC} | Avg Latency: ${latency_avg} ± ${latency_stddev}"
            results+=("{\"concurrency\":${concurrency},\"rps\":${rps},\"latency_avg\":\"${latency_avg}\"}")
        done

        # 保存
        printf "[%s]\n" "$(IFS=,; echo "${results[*]}")" > "${BENCH_DIR}/throughput.json"
        info "结果保存至 ${BENCH_DIR}/throughput.json"
    else
        warn "wrk 未安装，使用 curl 近似测量"
        local concurrency=10
        local duration=15
        local total_reqs=0

        echo -e "  并发: ${concurrency} | 持续时间: ${duration}s"

        for ((i=1; i<=concurrency; i++)); do
            (
                for ((j=1; j<=duration*5; j++)); do
                    curl -s -o /dev/null --max-time 5 "${BASE_URL}/health" 2>/dev/null || true
                done
            ) &
        done
        wait

        # 粗略估算 (不精确)
        echo -e "  提示: 安装 wrk 以获得精确结果"
        echo -e "  apt install wrk  # 或 brew install wrk"
    fi
}

# ============================================================
# 4. 延迟分布测量
# ============================================================
measure_latency() {
    header "4/4 延迟分布测量"

    local total=500
    local latencies=()

    echo -e "  发送 ${total} 次请求..."
    local progress=0

    for ((i=1; i<=total; i++)); do
        local start end elapsed
        start=$(date +%s%N)
        curl -s -o /dev/null -w "%{http_code}" --max-time 10 "${BASE_URL}/health" > /dev/null 2>&1 || true
        end=$(date +%s%N)
        elapsed=$(( (end - start) / 1000000 ))
        latencies+=("$elapsed")

        # 进度指示
        if [[ $((i * 100 / total)) -gt $progress ]]; then
            progress=$((i * 100 / total))
            echo -ne "  \r  进度: ${progress}% (${i}/${total})"
        fi
    done
    echo -e "\r  进度: 100% (${total}/${total})"

    # 排序统计
    local sorted=($(printf '%s\n' "${latencies[@]}" | sort -n))
    local count=${#sorted[@]}
    local sum=0
    for v in "${sorted[@]}"; do sum=$((sum + v)); done

    local min="${sorted[0]}"
    local max="${sorted[$((count - 1))]}"
    local avg=$((sum / count))
    local p50="${sorted[$((count * 50 / 100))]}"
    local p75="${sorted[$((count * 75 / 100))]}"
    local p90="${sorted[$((count * 90 / 100))]}"
    local p95="${sorted[$((count * 95 / 100))]}"
    local p99="${sorted[$((count * 99 / 100))]}"
    local p999="${sorted[$((count * 999 / 1000))]}"

    echo -e "\n  结果 (ms):"
    echo -e "    Min:    ${BLUE}${min}ms${NC}"
    echo -e "    Avg:    ${BLUE}${avg}ms${NC}"
    echo -e "    Max:    ${BLUE}${max}ms${NC}"
    echo -e "    P50:    ${BLUE}${p50}ms${NC}"
    echo -e "    P75:    ${BLUE}${p75}ms${NC}"
    echo -e "    P90:    ${BLUE}${p90}ms${NC}"
    echo -e "    P95:    ${BLUE}${p95}ms${NC}"
    echo -e "    P99:    ${BLUE}${p99}ms${NC}"
    echo -e "    P99.9:  ${BLUE}${p999}ms${NC}"

    cat > "${BENCH_DIR}/latency.json" <<JSON
{
  "metric": "latency_ms",
  "endpoint": "/health",
  "samples": $count,
  "min": $min,
  "avg": $avg,
  "max": $max,
  "p50": $p50,
  "p75": $p75,
  "p90": $p90,
  "p95": $p95,
  "p99": $p99,
  "p999": $p999,
  "all_values": [$(IFS=,; echo "${sorted[*]}")],
  "timestamp": "$(date -Iseconds)"
}
JSON
    info "结果保存至 ${BENCH_DIR}/latency.json"
}

# ============================================================
# 主流程
# ============================================================
main() {
    echo ""
    echo -e "${BOLD}╔══════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}║   LingShu Benchmark 基线测量 v4.2.7 LTS    ║${NC}"
    echo -e "${BOLD}╚══════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "  目标:     ${BASE_URL}"
    echo -e "  模式:     ${MODE}"
    echo -e "  输出目录: ${BENCH_DIR}"
    echo ""

    case "$MODE" in
        startup)    measure_startup;;
        memory)     measure_memory;;
        throughput) measure_throughput;;
        latency)    measure_latency;;
        all|*)
            measure_startup
            measure_memory
            measure_throughput
            measure_latency
            ;;
    esac

    # 汇总报告
    header "Benchmark 汇总"
    echo -e "  输出目录: ${BENCH_DIR}"
    echo -e "  文件列表:"
    ls -la "$BENCH_DIR" 2>/dev/null | awk 'NR>1{print "    " $NF}'

    # 生成 HTML 报告
    cat > "${BENCH_DIR}/report.html" <<HTML
<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<title>LingShu Benchmark 报告</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; max-width: 800px; margin: 40px auto; padding: 0 20px; }
h1 { color: #333; border-bottom: 2px solid #eee; padding-bottom: 10px; }
table { width: 100%; border-collapse: collapse; margin: 20px 0; }
th, td { padding: 10px; text-align: left; border-bottom: 1px solid #eee; }
th { background: #f5f5f5; }
.pass { color: green; }
.warn { color: orange; }
.fail { color: red; }
</style>
</head>
<body>
<h1>LingShu Benchmark 报告</h1>
<p>生成时间: $(date '+%Y-%m-%d %H:%M:%S')</p>
<p>目标: ${BASE_URL}</p>
<h2>测试指标</h2>
<table>
<tr><th>指标</th><th>值</th><th>文件</th></tr>
<tr><td>启动时间 (Avg)</td><td id="startup">-</td><td>startup.json</td></tr>
<tr><td>空闲内存 (Avg)</td><td id="idle_mem">-</td><td>memory.json</td></tr>
<tr><td>负载内存 (Avg)</td><td id="load_mem">-</td><td>memory.json</td></tr>
<tr><td>P50 延迟</td><td id="p50">-</td><td>latency.json</td></tr>
<tr><td>P95 延迟</td><td id="p95">-</td><td>latency.json</td></tr>
<tr><td>P99 延迟</td><td id="p99">-</td><td>latency.json</td></tr>
</table>
</body>
</html>
HTML
    info "HTML 报告: ${BENCH_DIR}/report.html"
    echo -e "${GREEN}✅ Benchmark 完成${NC}"
}

main "$@"
