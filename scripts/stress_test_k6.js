// =============================================================================
// Lingshu k6 压测脚本
// =============================================================================
// 使用: k6 run scripts/stress_test_k6.js
// 安装 k6: https://k6.io/docs/getting-started/installation/
// =============================================================================

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// ── 自定义指标 ──
const errorRate = new Rate('errors');
const healthLatency = new Trend('health_latency');
const chatLatency = new Trend('chat_latency');
const modelLatency = new Trend('model_latency');
const totalRequests = new Counter('total_requests');

// ── 配置 ──
const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';

export const options = {
    stages: [
        { duration: '30s', target: 10 },   // 逐步上升到 10 并发
        { duration: '1m', target: 50 },    // 上升到 50 并发
        { duration: '2m', target: 100 },   // 上升到 100 并发
        { duration: '1m', target: 100 },   // 维持 100 并发
        { duration: '30s', target: 0 },    // 逐渐下降
    ],
    thresholds: {
        errors: ['rate<0.05'],            // 错误率 < 5%
        http_req_duration: ['p(95)<2000'], // P95 < 2s
        health_latency: ['p(95)<500'],    // 健康检查 P95 < 500ms
    },
};

// ── 设置 ──
export function setup() {
    // 验证服务可用
    const res = http.get(`${BASE_URL}/health`);
    check(res, {
        '服务运行正常': (r) => r.status === 200,
    });
    console.log(`目标: ${BASE_URL}`);
    return { alive: true };
}

// ── 主流程 ──
export default function (data) {
    totalRequests.add(1);

    group('健康检查', function () {
        const res = http.get(`${BASE_URL}/health`);
        healthLatency.add(res.timings.duration);
        check(res, {
            'health 返回 200': (r) => r.status === 200,
        });
        errorRate.add(res.status !== 200);
    });

    group('列出模型', function () {
        const res = http.get(`${BASE_URL}/v1/models`);
        modelLatency.add(res.timings.duration);
        check(res, {
            'models 返回 200': (r) => r.status === 200,
        });
        errorRate.add(res.status !== 200);
    });

    // 10% 的请求做 Chat Completion
    if (__VU % 10 === 0) {
        group('Chat Completion', function () {
            const payload = JSON.stringify({
                model: 'mock',
                messages: [{ role: 'user', content: 'Hello, what can you do?' }],
                stream: false,
            });
            const params = {
                headers: { 'Content-Type': 'application/json' },
                timeout: '30s',
            };
            const res = http.post(`${BASE_URL}/v1/chat/completions`, payload, params);
            chatLatency.add(res.timings.duration);
            check(res, {
                'chat 返回 200': (r) => r.status === 200,
                '返回 choices': (r) => {
                    try {
                        return JSON.parse(r.body).choices !== undefined;
                    } catch {
                        return false;
                    }
                },
            });
            errorRate.add(res.status !== 200);
        });
    }

    // 随机等待 0.5-2 秒
    sleep(Math.random() * 1.5 + 0.5);
}

// ── 收尾 ──
export function teardown(data) {
    console.log('压测完成');
    console.log(`总请求数: ${totalRequests.name}`);
}
