#!/usr/bin/env bash
# =============================================================================
#  LingShu 生产压测脚本 v4.2.7
# =============================================================================
# 用法:
#   ./scripts/stress_test.sh                              # 默认快速压测 (30s)
#   ./scripts/stress_test.sh http://host:port              # 指定目标
#   ./scripts/stress_test.sh http://host:port 120          # 指定时长
#   ./scripts/stress_test.sh http://host:port 60 20        # 时长+并发
#   ./scripts/stress_test.sh --long 43200                  # 12h 稳定性测试
#   ./scripts/stress_test.sh --long 86400                  # 24h 稳定性测试
#   ./scripts/stress_test.sh --long 259200                 # 72h 稳定性测试
#   ./scripts/stress_test.sh --endpoints                   # 只测试 API 端点覆盖
#   ./scripts/stress_test.sh --memory                      # 只做内存泄漏检测
#
# 依赖:
#   - curl (基本)
#   - jq   (可选，用于输出格式化)
# =============================================================================

set -euo pipefail

# ── 配置 ──
BASE_URL="${1:-http://localhost:8080}"
DURATION="${2:-30}"
CONCURRENCY="${3:-10}"
MODE="quick"   # quick | long | endpoints | memory
LONG_DURATION=86400  # 默认 24h

# 解析参数
for arg in "$@"; do
    case "$arg" in
        --long)     MODE="long";   DURATION="${2:-86400}";;
        --endpoints) MODE="endpoints"; DURATION=0;;
        --memory)   MODE="memory"; DURATION=0;;
        --help|-h)
            echo "用法: $0 [url] [duration] [concurrency]"
            echo "      $0 --long [seconds]     # 长时间稳定性测试"
            echo "      $0 --endpoints           # API 端点覆盖测试"
            echo "      $0 --memory              # 内存泄漏检测"
            exit 0;;
    esac
done

# ── 颜色 ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
err()   { echo -e "${RED}[ERR]${NC} $*" >&2; }
header(){ echo -e "\n${BOLD}━━━ $* ━━━${NC}"; }
ok()    { echo -e "  ${GREEN}✔${NC} $*"; }
fail()  { echo -e "  ${RED}✘${NC} $*"; }

# ── 全局统计 ──
TOTAL_PASSED=0
TOTAL_FAILED=0
ALL_LATENCIES=()
START_TIME=""
END_TIME=""

# ── 前置检查 ──
check_deps() {
    local missing=0
    for cmd in curl; do
        if ! command -v "$cmd" &>/dev/null; then
            err "缺少依赖: $cmd"
            missing=$((missing + 1))
        fi
    done
    if [[ $missing -gt 0 ]]; then
        exit 1
    fi
}

# ── 请求函数 (统一计时 & 记录) ──
request() {
    local method="$1"
    local endpoint="$2"
    local payload="${3:-}"
    local timeout="${4:-10}"
    local extra_args="${5:-}"

    local start end elapsed http_code
    start=$(date +%s%N)

    if [[ "$method" == "GET" ]]; then
        http_code=$(curl -s -o /dev/null -w "%{http_code}" \
            --max-time "$timeout" \
            "${BASE_URL}${endpoint}" 2>/dev/null || echo "000")
    elif [[ "$method" == "POST" ]]; then
        http_code=$(curl -s -o /dev/null -w "%{http_code}" \
            -X POST \
            -H "Content-Type: application/json" \
            -d "${payload}" \
            --max-time "$timeout" \
            "${BASE_URL}${endpoint}" 2>/dev/null || echo "000")
    else
        http_code=$(curl -s -o /dev/null -w "%{http_code}" \
            -X "$method" \
            --max-time "$timeout" \
            "${BASE_URL}${endpoint}" 2>/dev/null || echo "000")
    fi

    end=$(date +%s%N)
    elapsed=$(( (end - start) / 1000000 ))
    ALL_LATENCIES+=("$elapsed")

    if [[ "$http_code" == "200" || "$http_code" == "201" || "$http_code" == "204" ]]; then
        TOTAL_PASSED=$((TOTAL_PASSED + 1))
        echo "ok"
    else
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        echo "fail"
    fi
}

# ── 健康检查 ──
health_check() {
    header "健康检查"
    local result
    result=$(request "GET" "/health" "" 5)
    if [[ "$result" == "ok" ]]; then
        ok "服务运行正常"
    else
        fail "服务异常 — 请先启动: ./start.sh"
        exit 1
    fi
}

# ── API 端点覆盖测试 ──
api_endpoint_coverage() {
    header "API 端点覆盖测试"

    local endpoints_health=(
        "GET:/health"
        "GET:/version"
        "GET:/v1/metrics"
        "GET:/v1/models"
    )
    local endpoints_agents=(
        "GET:/v1/agents"
    )
    local endpoints_plugins=(
        "GET:/v1/plugins"
    )
    local endpoints_federation=(
        "GET:/v1/federation/status"
        "GET:/v1/federation/nodes"
    )
    local endpoints_eval=(
        "GET:/v1/eval/result"
    )
    local endpoints_tenant=(
        "GET:/v1/tenant/orgs"
    )
    local endpoints_tee=(
        "GET:/v1/tee/health"
    )
    local endpoints_vault=(
        "GET:/v1/vault/health"
    )
    local endpoints_mcp=(
        "GET:/v1/mcp/tools"
    )
    local endpoints_watch=(
        "GET:/v1/watch/status"
    )
    local endpoints_files=(
        "GET:/v1/files"
    )

    local categories=(
        "系统:endpoints_health"
        "Agent:endpoints_agents"
        "插件:endpoints_plugins"
        "联邦:endpoints_federation"
        "评测:endpoints_eval"
        "租户:endpoints_tenant"
        "TEE:endpoints_tee"
        "Vault:endpoints_vault"
        "MCP:endpoints_mcp"
        "监控:endpoints_watch"
        "文件:endpoints_files"
    )

    for category in "${categories[@]}"; do
        local cat_name="${category%%:*}"
        local var_name="${category#*:}"
        local -n endpoints="$var_name"

        echo -e "\n  ${CYAN}[${cat_name}]${NC}"
        for ep_def in "${endpoints[@]}"; do
            local method="${ep_def%%:*}"
            local endpoint="${ep_def#*:}"
            local result
            result=$(request "$method" "$endpoint")
            if [[ "$result" == "ok" ]]; then
                ok "${method} ${endpoint}"
            else
                fail "${method} ${endpoint}"
            fi
        done
    done

    # POST 端点（使用 mock payload）
    header "POST 端点测试"
    local post_endpoints=(
        "POST:/v1/chat/completions:{\"model\":\"mock\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"stream\":false}"
        "POST:/v1/agent/run:{\"agent_id\":\"test\",\"task\":\"hello\"}"
    )
    for ep_def in "${post_endpoints[@]}"; do
        IFS=: read -r method endpoint payload <<< "$ep_def"
        local result
        result=$(request "$method" "$endpoint" "$payload" 30)
        if [[ "$result" == "ok" ]]; then
            ok "POST ${endpoint}"
        else
            fail "POST ${endpoint} (可能需 mock LLM 运行中)"
        fi
    done
}

# ── 单请求延迟分布 ──
single_request_test() {
    header "单请求延迟测试 (50 次 /health)"
    local total=50 passed=0 failed=0 total_time=0
    local latencies=()

    for ((i=1; i<=total; i++)); do
        local start end elapsed code
        start=$(date +%s%N)
        code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "${BASE_URL}/health" 2>/dev/null || echo "000")
        end=$(date +%s%N)
        elapsed=$(( (end - start) / 1000000 ))
        latencies+=("$elapsed")
        total_time=$((total_time + elapsed))
        if [[ "$code" == "200" ]]; then
            passed=$((passed + 1))
        else
            failed=$((failed + 1))
        fi
    done

    local sorted=($(printf '%s\n' "${latencies[@]}" | sort -n))
    local min="${sorted[0]}"
    local max="${sorted[$((total - 1))]}"
    local avg=$((total_time / total))
    local p50="${sorted[$((total * 50 / 100))]}"
    local p90="${sorted[$((total * 90 / 100))]}"
    local p95="${sorted[$((total * 95 / 100))]}"
    local p99="${sorted[$((total * 99 / 100))]}"

    echo -e "  总请求:     ${total}"
    echo -e "  通过:       ${GREEN}${passed}${NC}"
    echo -e "  失败:       ${RED}${failed}${NC}"
    echo -e "  成功率:     $(echo "scale=2; ${passed}*100/${total}" | bc)%"
    echo -e "  ── 延迟 (ms) ──"
    echo -e "  Min:  ${BLUE}${min}ms${NC}"
    echo -e "  Avg:  ${BLUE}${avg}ms${NC}"
    echo -e "  Max:  ${BLUE}${max}ms${NC}"
    echo -e "  P50:  ${BLUE}${p50}ms${NC}"
    echo -e "  P90:  ${BLUE}${p90}ms${NC}"
    echo -e "  P95:  ${BLUE}${p95}ms${NC}"
    echo -e "  P99:  ${BLUE}${p99}ms${NC}"
}

# ── 并发压测 ──
concurrent_test() {
    header "并发压测 (${CONCURRENCY} 并发 × ${DURATION}s)"

    local start_time end_time passed=0 failed=0 total_time=0
    local latencies=()
    local pid_list=()
    local tmp_dir
    tmp_dir=$(mktemp -d)

    worker() {
        local id=$1
        while true; do
            local start end elapsed code
            start=$(date +%s%N)
            code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 30 "${BASE_URL}/health" 2>/dev/null || echo "000")
            end=$(date +%s%N)
            elapsed=$(( (end - start) / 1000000 ))

            echo "$elapsed" >> "${tmp_dir}/latency_${id}"
            if [[ "$code" == "200" ]]; then
                echo "ok" >> "${tmp_dir}/result_${id}"
            else
                echo "fail" >> "${tmp_dir}/result_${id}"
            fi
        done
    }

    start_time=$(date +%s)
    for ((i=1; i<=CONCURRENCY; i++)); do
        worker "$i" &
        pid_list+=("$!")
    done

    sleep "$DURATION"

    for pid in "${pid_list[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true

    local total_reqs=0
    for ((i=1; i<=CONCURRENCY; i++)); do
        if [[ -f "${tmp_dir}/result_${i}" ]]; then
            while IFS= read -r line; do
                total_reqs=$((total_reqs + 1))
                if [[ "$line" == "ok" ]]; then
                    passed=$((passed + 1))
                else
                    failed=$((failed + 1))
                fi
            done < "${tmp_dir}/result_${i}"
        fi
        if [[ -f "${tmp_dir}/latency_${i}" ]]; then
            while IFS= read -r line; do
                latencies+=("$line")
                total_time=$((total_time + line))
            done < "${tmp_dir}/latency_${i}"
        fi
    done

    rm -rf "$tmp_dir"

    local elapsed_secs=$(( $(date +%s) - start_time ))
    local rps=0
    [[ $elapsed_secs -gt 0 ]] && rps=$((total_reqs / elapsed_secs))

    echo -e "  总请求:     ${total_reqs}"
    echo -e "  通过:       ${GREEN}${passed}${NC}"
    echo -e "  失败:       ${RED}${failed}${NC}"
    local error_pct=0
    [[ $total_reqs -gt 0 ]] && error_pct=$(echo "scale=2; ${failed}*100/${total_reqs}" | bc)
    echo -e "  错误率:     ${error_pct}%"
    echo -e "  RPS:        ${CYAN}${rps} req/s${NC}"

    if [[ ${#latencies[@]} -gt 0 ]]; then
        local sorted=($(printf '%s\n' "${latencies[@]}" | sort -n))
        local min="${sorted[0]}"
        local max="${sorted[$(( ${#sorted[@]} - 1 ))]}"
        local avg=$((total_time / ${#sorted[@]}))
        local p50="${sorted[$(( ${#sorted[@]} * 50 / 100 ))]}"
        local p90="${sorted[$(( ${#sorted[@]} * 90 / 100 ))]}"
        local p95="${sorted[$(( ${#sorted[@]} * 95 / 100 ))]}"
        local p99="${sorted[$(( ${#sorted[@]} * 99 / 100 ))]}"
        echo -e "  ── 延迟 (ms) ──"
        echo -e "  Min: ${min}ms | Avg: ${avg}ms | Max: ${max}ms"
        echo -e "  P50: ${p50}ms | P90: ${p90}ms | P95: ${p95}ms | P99: ${p99}ms"
    fi
}

# ── 长时间稳定性测试 ──
long_stability_test() {
    local duration=$LONG_DURATION
    local report_interval=3600  # 每小时报告一次
    local check_interval=60     # 每 60s 做一次健康检查
    local report_file="stability_report_$(date +%Y%m%d_%H%M%S).log"

    # 如果通过参数传入了时长，使用第一个非 --long 参数
    for arg in "$@"; do
        if [[ "$arg" =~ ^[0-9]+$ ]]; then
            duration=$arg
            break
        fi
    done

    header "长时间稳定性测试 (${duration}s = $(echo "scale=1; ${duration}/3600" | bc)h)"
    echo -e "  目标:       ${BASE_URL}"
    echo -e "  持续时间:   ${duration}s"
    echo -e "  报告间隔:   ${report_interval}s"
    echo -e "  检查间隔:   ${check_interval}s"
    echo -e "  日志文件:   ${report_file}"
    echo ""

    local start_ts=$(date +%s)
    local end_ts=$((start_ts + duration))
    local total_checks=0 passed_checks=0 failed_checks=0
    local total_latency=0 latency_samples=0
    local min_latency=999999 max_latency=0
    local last_report_ts=$start_ts
    local prev_passed=0 prev_failed=0

    # 初始化日志
    echo "==========================================" > "$report_file"
    echo "  LingShu 稳定性测试报告" >> "$report_file"
    echo "  开始时间: $(date -d @${start_ts} '+%Y-%m-%d %H:%M:%S')" >> "$report_file"
    echo "  目标:     ${BASE_URL}" >> "$report_file"
    echo "  时长:     ${duration}s" >> "$report_file"
    echo "==========================================" >> "$report_file"

    while true; do
        local now=$(date +%s)
        if [[ $now -ge $end_ts ]]; then
            break
        fi

        # 健康检查
        local start end elapsed code
        start=$(date +%s%N)
        code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "${BASE_URL}/health" 2>/dev/null || echo "000")
        end=$(date +%s%N)
        elapsed=$(( (end - start) / 1000000 ))

        total_checks=$((total_checks + 1))
        total_latency=$((total_latency + elapsed))
        latency_samples=$((latency_samples + 1))
        [[ $elapsed -lt $min_latency ]] && min_latency=$elapsed
        [[ $elapsed -gt $max_latency ]] && max_latency=$elapsed

        if [[ "$code" == "200" ]]; then
            passed_checks=$((passed_checks + 1))
        else
            failed_checks=$((failed_checks + 1))
            echo "[$(date '+%H:%M:%S')] 健康检查失败: HTTP ${code}" >> "$report_file"
        fi

        # 定期报告
        if [[ $((now - last_report_ts)) -ge $report_interval ]]; then
            local elapsed_secs=$((now - start_ts))
            local remain=$((end_ts - now))
            local avg_latency=0
            [[ $latency_samples -gt 0 ]] && avg_latency=$((total_latency / latency_samples))
            local success_rate=$(echo "scale=2; ${passed_checks}*100/${total_checks}" | bc)
            local current_rps=$(( (passed_checks - prev_passed) / report_interval ))
            local current_errors=$((failed_checks - prev_failed))

            local report="[$(date '+%H:%M:%S')] 已运行 ${elapsed_secs}s / 剩余 ${remain}s | "
            report+="检查 ${total_checks} 次 | 通过 ${passed_checks} | 失败 ${failed_checks} | "
            report+="成功率 ${success_rate}% | Avg延迟 ${avg_latency}ms | "
            report+="当前RPS ${current_rps} | 当前错误 ${current_errors}"

            echo -e "  ${report}"
            echo "${report}" >> "$report_file"

            prev_passed=$passed_checks
            prev_failed=$failed_checks
            last_report_ts=$now
        fi

        sleep "$check_interval"
    done

    # 最终统计
    local total_elapsed=$((now - start_ts))
    local avg_latency=0
    [[ $latency_samples -gt 0 ]] && avg_latency=$((total_latency / latency_samples))
    local success_rate=$(echo "scale=2; ${passed_checks}*100/${total_checks}" | bc)
    local overall_rps=$((total_checks / total_elapsed))

    echo "" | tee -a "$report_file"
    echo -e "${BOLD}━━━ 稳定性测试结果 ━━━${NC}" | tee -a "$report_file"
    echo "  目标:       ${BASE_URL}" | tee -a "$report_file"
    echo "  实际运行:   ${total_elapsed}s" | tee -a "$report_file"
    echo "  总检查:     ${total_checks}" | tee -a "$report_file"
    echo "  通过:       ${passed_checks}" | tee -a "$report_file"
    echo "  失败:       ${failed_checks}" | tee -a "$report_file"
    echo "  成功率:     ${success_rate}%" | tee -a "$report_file"
    echo "  平均 RPS:   ${overall_rps}" | tee -a "$report_file"
    echo "  延迟 Min:   ${min_latency}ms" | tee -a "$report_file"
    echo "  延迟 Avg:   ${avg_latency}ms" | tee -a "$report_file"
    echo "  延迟 Max:   ${max_latency}ms" | tee -a "$report_file"

    local verdict="✅ 通过"
    if [[ $failed_checks -gt 0 ]]; then
        local error_pct=$(echo "scale=2; ${failed_checks}*100/${total_checks}" | bc)
        if (( $(echo "$error_pct > 1.0" | bc -l) )); then
            verdict="${RED}✘ 失败 (错误率 ${error_pct}% > 1%)${NC}"
        else
            verdict="${YELLOW}⚠ 需关注 (错误率 ${error_pct}%)${NC}"
        fi
    fi
    echo -e "  结论:       ${verdict}" | tee -a "$report_file"
    echo "  完整日志:   ${report_file}" | tee -a "$report_file"
}

# ── 内存泄漏检测 ──
memory_leak_test() {
    header "内存泄漏检测"

    local pid
    pid=$(pgrep -f "lingshu" | head -1 2>/dev/null || echo "")

    if [[ -z "$pid" ]]; then
        warn "未找到 lingshu 进程，跳过内存检测"
        return
    fi

    echo -e "  监控进程:   lingshu (PID: ${pid})"
    echo ""

    # 使用 /proc/$pid/status 获取 VmRSS
    local samples=10
    local interval=5
    local mem_values=()
    local mem_file="/proc/${pid}/status"

    if [[ ! -f "$mem_file" ]]; then
        warn "无法读取 ${mem_file}，尝试使用 ps"
        # 备选方案
        for ((i=1; i<=samples; i++)); do
            local mem
            mem=$(ps -o rss= -p "$pid" 2>/dev/null || echo "0")
            mem_values+=("${mem// /}")
            echo -e "  采样 ${i}/${samples}: RSS = ${mem} KB"
            sleep "$interval"
        done
    else
        for ((i=1; i<=samples; i++)); do
            local mem
            mem=$(grep -i "VmRSS" "$mem_file" 2>/dev/null | awk '{print $2}' || echo "N/A")
            mem_values+=("${mem}")
            local mem_mb="N/A"
            [[ "$mem" != "N/A" ]] && mem_mb=$(echo "scale=1; ${mem}/1024" | bc)
            echo -e "  采样 ${i}/${samples}: RSS = ${mem_mb} MB"
            sleep "$interval"
        done
    fi

    echo ""
    # 分析内存趋势
    if [[ ${#mem_values[@]} -ge 3 ]]; then
        local first="${mem_values[0]}"
        local last="${mem_values[$((samples - 1))]}"
        local diff=$((last - first))

        echo -e "  初始 RSS: ${first} KB"
        echo -e "  最终 RSS: ${last} KB"
        echo -e "  差值:     ${diff} KB"

        if [[ $diff -gt 0 ]]; then
            local growth_rate=$(echo "scale=2; ${diff}/${samples}" | bc)
            echo -e "  增长速率: ${growth_rate} KB/采样"
            if (( $(echo "$growth_rate > 100" | bc -l) )); then
                echo -e "  ${RED}⚠ 可能存在内存泄漏 (每采样增长 >100 KB)${NC}"
            else
                echo -e "  ${YELLOW}⚠ 轻微增长 (每采样 ${growth_rate} KB)${NC}"
            fi
        else
            echo -e "  ${GREEN}✅ 内存稳定 (无增长)${NC}"
        fi
    fi

    # 使用 /proc/meminfo 查看系统内存
    if [[ -f /proc/meminfo ]]; then
        echo ""
        echo -e "  系统内存状态:"
        local mem_total=$(grep "MemTotal" /proc/meminfo | awk '{print $2}')
        local mem_avail=$(grep "MemAvailable" /proc/meminfo | awk '{print $2}')
        local mem_free=$(grep "MemFree" /proc/meminfo | awk '{print $2}')
        local usage_pct=$(echo "scale=1; (${mem_total}-${mem_avail})*100/${mem_total}" | bc)
        echo -e "    总计:  $(echo "scale=1; ${mem_total}/1024" | bc) MB"
        echo -e "    可用:  $(echo "scale=1; ${mem_avail}/1024" | bc) MB"
        echo -e "    使用率: ${usage_pct}%"
    fi
}

# ── 资源监控 ──
resource_monitor() {
    header "资源使用监控 (${DURATION}s)"

    local pid
    pid=$(pgrep -f "lingshu" | head -1 2>/dev/null || echo "")

    if [[ -n "$pid" ]]; then
        echo -e "  监控进程: lingshu (PID: ${pid})"
        local cpu_before mem_before cpu_after mem_after
        cpu_before=$(ps -p "$pid" -o %cpu --no-headers 2>/dev/null || echo "0")
        mem_before=$(ps -p "$pid" -o %mem --no-headers 2>/dev/null || echo "0")
        sleep "$DURATION"
        cpu_after=$(ps -p "$pid" -o %cpu --no-headers 2>/dev/null || echo "0")
        mem_after=$(ps -p "$pid" -o %mem --no-headers 2>/dev/null || echo "0")
        echo -e "  CPU:  ${cpu_before}% → ${cpu_after}%"
        echo -e "  MEM:  ${mem_before}% → ${mem_after}%"
    else
        warn "未找到 lingshu 进程，跳过资源监控"
    fi

    if command -v free &>/dev/null; then
        echo -e "  系统内存:"
        free -h | awk 'NR==1{print "            " $0} NR==2{print "  " $0}'
    fi
}

# ── 生成报告 ──
generate_report() {
    header "压测报告摘要"

    local total=$((TOTAL_PASSED + TOTAL_FAILED))
    local error_pct=0
    [[ $total -gt 0 ]] && error_pct=$(echo "scale=2; ${TOTAL_FAILED}*100/${total}" | bc)

    echo -e "  目标:       ${BASE_URL}"
    echo -e "  模式:       ${MODE}"
    [[ "$MODE" != "endpoints" && "$MODE" != "memory" ]] && echo -e "  持续时间:   ${DURATION}s"
    [[ "$MODE" != "endpoints" && "$MODE" != "memory" ]] && echo -e "  并发:       ${CONCURRENCY}"
    echo -e "  时间:       $(date '+%Y-%m-%d %H:%M:%S')"

    if [[ $total -gt 0 ]]; then
        echo -e "  总请求:     ${total}"
        echo -e "  通过:       ${GREEN}${TOTAL_PASSED}${NC}"
        echo -e "  失败:       ${RED}${TOTAL_FAILED}${NC}"
        echo -e "  错误率:     ${error_pct}%"
    fi

    # 延迟统计
    if [[ ${#ALL_LATENCIES[@]} -gt 1 ]]; then
        local sorted=($(printf '%s\n' "${ALL_LATENCIES[@]}" | sort -n))
        local count=${#sorted[@]}
        local sum=0
        for v in "${sorted[@]}"; do sum=$((sum + v)); done
        local avg=$((sum / count))
        local min="${sorted[0]}"
        local max="${sorted[$((count - 1))]}"
        local p50="${sorted[$((count * 50 / 100))]}"
        local p90="${sorted[$((count * 90 / 100))]}"
        local p95="${sorted[$((count * 95 / 100))]}"
        local p99="${sorted[$((count * 99 / 100))]}"

        echo -e "  ── 延迟 (ms) ──"
        echo -e "  Min: ${min} | Avg: ${avg} | Max: ${max}"
        echo -e "  P50: ${p50} | P90: ${p90} | P95: ${p95} | P99: ${p99}"
    fi

    echo ""
    echo -e "  ${BOLD}评估标准${NC}"
    local p99_val="${sorted[$(( ${#ALL_LATENCIES[@]} * 99 / 100 ))]:-0}"
    if [[ $p99_val -lt 500 ]]; then
        echo -e "  P99 延迟 < 500ms  ${GREEN}✅ 优秀${NC}"
    elif [[ $p99_val -lt 1000 ]]; then
        echo -e "  P99 延迟 < 1000ms ${YELLOW}⚠ 可接受${NC}"
    else
        echo -e "  P99 延迟 > 1000ms ${RED}✘ 需优化${NC}"
    fi

    if (( $(echo "$error_pct < 1.0" | bc -l) )); then
        echo -e "  错误率 < 1%      ${GREEN}✅ 健康${NC}"
    elif (( $(echo "$error_pct < 5.0" | bc -l) )); then
        echo -e "  错误率 1-5%      ${YELLOW}⚠ 需关注${NC}"
    else
        echo -e "  错误率 > 5%      ${RED}✘ 严重${NC}"
    fi
}

# ── 主流程 ──
main() {
    echo ""
    echo -e "${BOLD}╔══════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}║     LingShu 生产压测 v4.2.7 LTS            ║${NC}"
    echo -e "${BOLD}╚══════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "  目标:     ${BASE_URL}"
    echo -e "  模式:     ${MODE}"
    [[ "$MODE" != "endpoints" && "$MODE" != "memory" ]] && echo -e "  持续时间: ${DURATION}s"
    [[ "$MODE" != "endpoints" && "$MODE" != "memory" ]] && echo -e "  并发:     ${CONCURRENCY}"
    echo ""

    check_deps
    START_TIME=$(date +%s)

    case "$MODE" in
        long)
            health_check
            long_stability_test "$@"
            ;;
        endpoints)
            health_check
            api_endpoint_coverage
            ;;
        memory)
            memory_leak_test
            ;;
        quick|*)
            health_check
            single_request_test
            concurrent_test
            api_endpoint_coverage
            resource_monitor
            generate_report
            ;;
    esac

    END_TIME=$(date +%s)
    local total_elapsed=$((END_TIME - START_TIME))
    echo ""
    echo -e "${GREEN}✅ 压测完成 (耗时 ${total_elapsed}s)${NC}"
}

main "$@"
