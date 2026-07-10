#!/usr/bin/env bash
# =============================================================================
#  Lingshu 生产压测脚本
# =============================================================================
# 用法:
#   ./scripts/stress_test.sh                    # 默认压测 localhost:8080
#   ./scripts/stress_test.sh http://host:port   # 指定目标地址
#   ./scripts/stress_test.sh http://host:port 60   # 指定持续秒数
#
# 依赖:
#   - curl (基本)
#   - jq   (可选，用于输出格式化)
# =============================================================================

set -euo pipefail

BASE_URL="${1:-http://localhost:8080}"
DURATION="${2:-30}"
CONCURRENCY="${3:-10}"
TOTAL_REQUESTS=$((DURATION * CONCURRENCY))

# ── 颜色 ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
err()   { echo -e "${RED}[ERR]${NC} $*" >&2; }
header(){ echo -e "\n${BOLD}━━━ $* ━━━${NC}"; }

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

# ── 健康检查 ──
health_check() {
    header "1/6 健康检查"
    local start end elapsed status
    start=$(date +%s%N)
    status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$BASE_URL/health" 2>/dev/null || echo "000")
    end=$(date +%s%N)
    elapsed=$(( (end - start) / 1000000 ))
    if [[ "$status" == "200" ]]; then
        echo -e "  ${GREEN}✔${NC} 服务运行正常 (${elapsed}ms, HTTP ${status})"
    else
        echo -e "  ${RED}✘${NC} 服务异常 (${elapsed}ms, HTTP ${status})"
        echo -e "  ${YELLOW}  请先启动: ./start.sh${NC}"
        exit 1
    fi
}

# ── 单请求测试 (延迟分布) ──
single_request_test() {
    header "2/6 单请求延迟测试"
    local total=50 passed=0 failed=0 total_time=0
    local latencies=()

    for ((i=1; i<=total; i++)); do
        local start end elapsed
        start=$(date +%s%N)
        local code
        code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$BASE_URL/health" 2>/dev/null || echo "000")
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

    # 计算统计
    local sorted
    sorted=($(printf '%s\n' "${latencies[@]}" | sort -n))
    local min="${sorted[0]}"
    local max="${sorted[$((total - 1))]}"
    local avg=$((total_time / total))
    local p50="${sorted[$((total * 50 / 100))]}"
    local p90="${sorted[$((total * 90 / 100))]}"
    local p99="${sorted[$((total * 99 / 100))]}"

    echo -e "  总请求:     ${total}"
    echo -e "  通过:       ${GREEN}${passed}${NC}"
    echo -e "  失败:       ${RED}${failed}${NC}"
    echo -e "  ── 延迟 (ms) ──"
    echo -e "  Min:        ${min}ms"
    echo -e "  Avg:        ${avg}ms"
    echo -e "  Max:        ${max}ms"
    echo -e "  P50:        ${p50}ms"
    echo -e "  P90:        ${p90}ms"
    echo -e "  P99:        ${p99}ms"
}

# ── 并发压测 ──
concurrent_test() {
    header "3/6 并发压测 (${CONCURRENCY} 并发 × ${DURATION}s)"

    local start_time end_time passed=0 failed=0 total_time=0
    local latencies=()
    local pid_list=()
    local tmp_dir
    tmp_dir=$(mktemp -d)
    local count_file="${tmp_dir}/count"

    echo 0 > "$count_file"

    worker() {
        local id=$1
        while true; do
            local start end elapsed code
            start=$(date +%s%N)
            code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 30 "$BASE_URL/health" 2>/dev/null || echo "000")
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

    # 启动 worker
    start_time=$(date +%s)
    for ((i=1; i<=CONCURRENCY; i++)); do
        worker "$i" &
        pid_list+=("$!")
    done

    # 等待
    sleep "$DURATION"

    # 停止 worker
    for pid in "${pid_list[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true

    # 汇总结果
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

    # 统计
    local elapsed_secs=$(( $(date +%s) - start_time ))
    local rps=0
    if [[ $elapsed_secs -gt 0 ]]; then
        rps=$((total_reqs / elapsed_secs))
    fi

    echo -e "  总请求:     ${total_reqs}"
    echo -e  "  通过:       ${GREEN}${passed}${NC}"
    echo -e "  失败:       ${RED}${failed}${NC}"
    echo -e "  RPS:        ${rps} req/s"

    if [[ ${#latencies[@]} -gt 0 ]]; then
        local sorted
        sorted=($(printf '%s\n' "${latencies[@]}" | sort -n))
        local min="${sorted[0]}"
        local max="${sorted[$(( ${#sorted[@]} - 1 ))]}"
        local avg=$((total_time / ${#sorted[@]}))
        local p50="${sorted[$(( ${#sorted[@]} * 50 / 100 ))]}"
        local p90="${sorted[$(( ${#sorted[@]} * 90 / 100 ))]}"
        local p99="${sorted[$(( ${#sorted[@]} * 99 / 100 ))]}"
        echo -e "  ── 延迟 (ms) ──"
        echo -e "  Min:  ${min}ms | Avg: ${avg}ms | Max: ${max}ms"
        echo -e "  P50:  ${p50}ms | P90: ${p90}ms | P99: ${p99}ms"
    fi
}

# ── API 端点压测 ──
api_endpoint_test() {
    header "4/6 API 端点压测"

    local endpoints=(
        "/health"
        "/v1/models"
    )

    for endpoint in "${endpoints[@]}"; do
        local passed=0 failed=0 total_time=0
        local latencies=()
        for ((i=1; i<=20; i++)); do
            local start end elapsed code
            start=$(date +%s%N)
            code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "${BASE_URL}${endpoint}" 2>/dev/null || echo "000")
            end=$(date +%s%N)
            elapsed=$(( (end - start) / 1000000 ))
            latencies+=("$elapsed")
            total_time=$((total_time + elapsed))
            [[ "$code" == "200" ]] && passed=$((passed + 1)) || failed=$((failed + 1))
        done
        local sorted=($(printf '%s\n' "${latencies[@]}" | sort -n))
        local avg=$((total_time / 20))
        local p90="${sorted[$((20 * 90 / 100))]}"
        echo -e "  ${BLUE}GET ${endpoint}${NC}"
        echo -e "    通过: ${passed}  失败: ${failed}  Avg: ${avg}ms  P90: ${p90}ms"
    done

    # Chat Completion 压测 (mock LLM)
    local endpoint="/v1/chat/completions"
    local payload='{"model":"mock","messages":[{"role":"user","content":"hello"}],"stream":false}'
    local passed=0 failed=0 total_time=0 latencies=()
    for ((i=1; i<=10; i++)); do
        local start end elapsed code
        start=$(date +%s%N)
        code=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
            -H "Content-Type: application/json" \
            -d "$payload" \
            --max-time 30 "${BASE_URL}${endpoint}" 2>/dev/null || echo "000")
        end=$(date +%s%N)
        elapsed=$(( (end - start) / 1000000 ))
        latencies+=("$elapsed")
        total_time=$((total_time + elapsed))
        [[ "$code" == "200" ]] && passed=$((passed + 1)) || failed=$((failed + 1))
    done
    local sorted=($(printf '%s\n' "${latencies[@]}" | sort -n))
    local avg=$((total_time / 10))
    local p90="${sorted[$((10 * 90 / 100))]}"
    echo -e "  ${BLUE}POST ${endpoint}${NC} (mock LLM)"
    echo -e "    通过: ${passed}  失败: ${failed}  Avg: ${avg}ms  P90: ${p90}ms"
}

# ── 资源监控 ──
resource_monitor() {
    header "5/6 资源使用监控 (${DURATION}s)"

    if command -v ps &>/dev/null; then
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
            echo -e "  ${YELLOW}⚠ 未找到 lingshu 进程，跳过资源监控${NC}"
        fi
    fi

    if command -v free &>/dev/null; then
        echo -e "  系统内存:"
        free -h | awk 'NR==1{print "            " $0} NR==2{print "  " $0}'
    fi
}

# ── 综合报告 ──
generate_report() {
    header "6/6 压测报告摘要"
    echo -e "  目标:       ${BASE_URL}"
    echo -e "  并发:       ${CONCURRENCY}"
    echo -e "  持续时间:   ${DURATION}s"
    echo -e "  时间:       $(date '+%Y-%m-%d %H:%M:%S')"
    echo ""
    echo -e "  ${BOLD}建议阈值${NC}"
    echo -e "  P99 延迟 < 500ms    ${GREEN}✅ 优秀${NC}"
    echo -e "  P99 延迟 < 1000ms   ${YELLOW}⚠ 可接受${NC}"
    echo -e "  P99 延迟 > 1000ms   ${RED}✘ 需优化${NC}"
    echo -e "  错误率 < 1%        ${GREEN}✅ 健康${NC}"
    echo -e "  错误率 1-5%        ${YELLOW}⚠ 需关注${NC}"
    echo -e "  错误率 > 5%        ${RED}✘ 严重${NC}"
}

# ── 主流程 ──
main() {
    echo ""
    echo -e "${BOLD}╔══════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}║     Lingshu 生产压测 (Stress Test)          ║${NC}"
    echo -e "${BOLD}╚══════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "  目标:     ${BASE_URL}"
    echo -e "  持续时间: ${DURATION}s"
    echo -e "  并发:     ${CONCURRENCY}"
    echo ""

    check_deps
    health_check
    single_request_test
    concurrent_test
    api_endpoint_test
    resource_monitor
    generate_report

    echo ""
    echo -e "${GREEN}✅ 压测完成${NC}"
}

main "$@"
