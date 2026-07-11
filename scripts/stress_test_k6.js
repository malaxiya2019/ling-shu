// =============================================================================
// LingShu k6 压测脚本 v4.2.7 LTS
// =============================================================================
// 使用:
//   k6 run scripts/stress_test_k6.js                              # 默认场景
//   k6 run -e BASE_URL=http://host:8080 scripts/stress_test_k6.js # 自定义目标
//   k6 run -e SCENARIO=long scripts/stress_test_k6.js             # 长时间测试
//   k6 run -e SCENARIO=spike scripts/stress_test_k6.js            # 尖峰测试
//   k6 run -e SCENARIO=endurance scripts/stress_test_k6.js        # 耐力测试
//   k6 run -e SCENARIO=endpoints scripts/stress_test_k6.js        # 端点覆盖
//   k6 run -e SCENARIO=smoke scripts/stress_test_k6.js            # 冒烟测试
//
// 安装 k6: https://k6.io/docs/getting-started/installation/
// =============================================================================

import http from 'k6/http';
import { check, sleep, group, fail } from 'k6';
import { Rate, Trend, Counter, Gauge } from 'k6/metrics';

// ── 自定义指标 ──
const errorRate = new Rate('errors');
const healthLatency = new Trend('health_latency');
const chatLatency = new Trend('chat_latency');
const modelLatency = new Trend('model_latency');
const agentLatency = new Trend('agent_latency');
const pluginLatency = new Trend('plugin_latency');
const federationLatency = new Trend('federation_latency');
const evalLatency = new Trend('eval_latency');
const tenantLatency = new Trend('tenant_latency');
const vaultLatency = new Trend('vault_latency');
const teeLatency = new Trend('tee_latency');
const mcpLatency = new Trend('mcp_latency');
const totalRequests = new Counter('total_requests');
const activeVUs = new Gauge('active_vus');

// ── 配置 ──
const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';
const SCENARIO = __ENV.SCENARIO || 'standard';

// ── 场景配置 ──
const scenarios = {
    // 标准压测: 逐步上升 → 维持 → 下降
    standard: {
        executor: 'ramping-arrival-rate',
        startRate: 10,
        timeUnit: '1s',
        preAllocatedVUs: 20,
        maxVUs: 200,
        stages: [
            { duration: '30s', target: 20 },    // 热身
            { duration: '1m', target: 50 },     // 上升
            { duration: '2m', target: 100 },    // 峰值
            { duration: '1m', target: 100 },    // 维持
            { duration: '30s', target: 0 },     // 下降
        ],
    },
    // 长时间耐力测试 (8h)
    endurance: {
        executor: 'constant-arrival-rate',
        rate: 50,
        timeUnit: '1s',
        duration: '8h',
        preAllocatedVUs: 50,
        maxVUs: 100,
    },
    // 尖峰测试
    spike: {
        executor: 'ramping-arrival-rate',
        startRate: 10,
        timeUnit: '1s',
        preAllocatedVUs: 20,
        maxVUs: 500,
        stages: [
            { duration: '2m', target: 10 },
            { duration: '10s', target: 500 },   // 瞬间尖峰
            { duration: '3m', target: 500 },    // 维持尖峰
            { duration: '30s', target: 10 },    // 恢复
            { duration: '1m', target: 0 },
        ],
    },
    // 冒烟测试 (低负载验证)
    smoke: {
        executor: 'constant-vus',
        vus: 3,
        duration: '1m',
    },
    // API 端点覆盖测试
    endpoints: {
        executor: 'constant-vus',
        vus: 5,
        duration: '2m',
    },
};

// ── 阈值配置 ──
const thresholds = {
    standard: {
        errors: ['rate<0.01'],              // 错误率 < 1%
        http_req_duration: ['p(95)<2000'],   // P95 < 2s
        health_latency: ['p(95)<500'],       // 健康检查 P95 < 500ms
        chat_latency: ['p(95)<10000'],       // Chat P95 < 10s (含 LLM 调用)
    },
    endurance: {
        errors: ['rate<0.005'],             // 错误率 < 0.5%
        http_req_duration: ['p(95)<3000'],
        health_latency: ['p(95)<1000'],
    },
    spike: {
        errors: ['rate<0.05'],              // 尖峰允许稍高错误率
        http_req_duration: ['p(95)<5000'],
    },
    smoke: {
        errors: ['rate<0.01'],
        http_req_duration: ['p(95)<1000'],
    },
    endpoints: {
        errors: ['rate<0.02'],
        http_req_duration: ['p(95)<3000'],
    },
};

// ── 设置选项 ──
const selectedScenario = scenarios[SCENARIO] || scenarios.standard;
const selectedThresholds = thresholds[SCENARIO] || thresholds.standard;

export const options = {
    scenarios: {
        main: selectedScenario,
    },
    thresholds: selectedThresholds,
};

// ── 设置 ──
export function setup() {
    const res = http.get(`${BASE_URL}/health`);
    check(res, {
        '服务运行正常': (r) => r.status === 200,
    });
    console.log(`[${SCENARIO}] 目标: ${BASE_URL}`);
    console.log(`[${SCENARIO}] VUs: ${selectedScenario.maxVUs || selectedScenario.vus || 'auto'}`);
    return { alive: true };
}

// ── 辅助: 标记检查 ──
function checkResponse(name, res, expectedStatus = 200) {
    const passed = check(res, {
        [`${name} 返回 ${expectedStatus}`]: (r) => r.status === expectedStatus,
    });
    errorRate.add(!passed);
    return passed;
}

// ── 辅助: POST 请求 ──
function postJSON(endpoint, body, timeout = '30s') {
    return http.post(`${BASE_URL}${endpoint}`, JSON.stringify(body), {
        headers: { 'Content-Type': 'application/json' },
        timeout: timeout,
    });
}

// ── API 端点分组 ──

function testHealth() {
    group('健康检查', function () {
        const res = http.get(`${BASE_URL}/health`);
        healthLatency.add(res.timings.duration);
        checkResponse('health', res);
    });
}

function testSystem() {
    group('系统端点', function () {
        let res = http.get(`${BASE_URL}/version`);
        checkResponse('version', res);

        res = http.get(`${BASE_URL}/v1/metrics`);
        checkResponse('metrics', res);

        res = http.get(`${BASE_URL}/v1/models`);
        modelLatency.add(res.timings.duration);
        checkResponse('models', res);
    });
}

function testChat() {
    group('Chat Completion (mock)', function () {
        const payload = {
            model: 'mock',
            messages: [{ role: 'user', content: 'Hello, what can you do?' }],
            stream: false,
        };
        const res = postJSON('/v1/chat/completions', payload, '30s');
        chatLatency.add(res.timings.duration);
        if (checkResponse('chat', res)) {
            try {
                const body = JSON.parse(res.body);
                check(res, {
                    '返回 choices': () => body.choices !== undefined,
                });
            } catch (e) {
                // response 可能为空（mock 模式）
            }
        }
    });
}

function testAgents() {
    group('Agent 管理', function () {
        let res = http.get(`${BASE_URL}/v1/agents`);
        agentLatency.add(res.timings.duration);
        checkResponse('agent list', res);

        // Agent run (可能失败如果 mock LLM 未就绪)
        const payload = {
            agent_id: 'test-agent',
            task: 'say hello',
            max_steps: 1,
        };
        res = postJSON('/v1/agent/run', payload, '30s');
        agentLatency.add(res.timings.duration);
        // 不标记为错误，仅记录
    });
}

function testPlugins() {
    group('插件系统', function () {
        let res = http.get(`${BASE_URL}/v1/plugins`);
        pluginLatency.add(res.timings.duration);
        checkResponse('plugins list', res);
    });
}

function testFederation() {
    group('联邦通信', function () {
        let res = http.get(`${BASE_URL}/v1/federation/status`);
        federationLatency.add(res.timings.duration);
        checkResponse('federation status', res);

        res = http.get(`${BASE_URL}/v1/federation/nodes`);
        federationLatency.add(res.timings.duration);
        checkResponse('federation nodes', res);
    });
}

function testEval() {
    group('评测', function () {
        const res = http.get(`${BASE_URL}/v1/eval/result`);
        evalLatency.add(res.timings.duration);
        checkResponse('eval result', res);
    });
}

function testTenant() {
    group('多租户', function () {
        const res = http.get(`${BASE_URL}/v1/tenant/orgs`);
        tenantLatency.add(res.timings.duration);
        checkResponse('tenant orgs', res);
    });
}

function testTEE() {
    group('TEE 安全', function () {
        const res = http.get(`${BASE_URL}/v1/tee/health`);
        teeLatency.add(res.timings.duration);
        checkResponse('tee health', res);
    });
}

function testVault() {
    group('Vault 密钥', function () {
        const res = http.get(`${BASE_URL}/v1/vault/health`);
        vaultLatency.add(res.timings.duration);
        checkResponse('vault health', res);
    });
}

function testMCP() {
    group('MCP 协议', function () {
        const res = http.get(`${BASE_URL}/v1/mcp/tools`);
        mcpLatency.add(res.timings.duration);
        checkResponse('mcp tools', res);
    });
}

function testFiles() {
    group('文件管理', function () {
        const res = http.get(`${BASE_URL}/v1/files`);
        checkResponse('files list', res);
    });
}

// ── 主流程 ──
export default function (data) {
    totalRequests.add(1);
    activeVUs.add(__VU);

    // 健康检查 (每次必做)
    testHealth();

    if (SCENARIO === 'endpoints') {
        // 端点覆盖场景: 遍历所有端点
        testSystem();
        testChat();
        testAgents();
        testPlugins();
        testFederation();
        testEval();
        testTenant();
        testTEE();
        testVault();
        testMCP();
        testFiles();
    } else if (SCENARIO === 'smoke') {
        // 冒烟测试: 只做核心端点
        testSystem();
        testChat();
    } else {
        // 标准/耐力/尖峰: 按概率分布
        const rand = Math.random();

        if (rand < 0.30) {
            // 30%: 系统端点
            testSystem();
        } else if (rand < 0.50) {
            // 20%: Chat (10% 概率做)
            if (__VU % 5 === 0) {
                testChat();
            } else {
                testSystem();
            }
        } else if (rand < 0.65) {
            // 15%: Agent
            testAgents();
        } else if (rand < 0.75) {
            // 10%: 插件
            testPlugins();
        } else if (rand < 0.85) {
            // 10%: 联邦
            testFederation();
        } else if (rand < 0.90) {
            // 5%: 评测
            testEval();
        } else {
            // 10%: 混合 (tenant/vault/tee/mcp/files)
            const subrand = Math.random();
            if (subrand < 0.25) testTenant();
            else if (subrand < 0.50) testVault();
            else if (subrand < 0.75) testTEE();
            else testMCP();
        }
    }

    // 随机等待 (模拟真实用户行为)
    if (SCENARIO !== 'endpoints' && SCENARIO !== 'smoke') {
        sleep(Math.random() * 2 + 0.5);
    } else {
        sleep(0.5);
    }
}

// ── 收尾 ──
export function teardown(data) {
    console.log(`[${SCENARIO}] 压测完成`);
}
