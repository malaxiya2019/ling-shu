# LingShu v5.0.0 发布说明

## 三大核心新能力

### 🐝 AgentSwarm (lingshu-swarm) — 群体智能协作引擎

多 Agent 协作框架，支持多种协作策略与动态拓扑。

```
┌─────────────────────────────────────────────────┐
│                  SwarmEngine                      │
│  ┌────────────┐  ┌──────────┐  ┌─────────────┐ │
│  │ Coordinator │  │ Channel  │  │ Topology     │ │
│  │ (协调器)    │  │ (通信)   │  │ (拓扑管理)   │ │
│  └────────────┘  └──────────┘  └─────────────┘ │
│  ┌────────────┐  ┌──────────┐  ┌─────────────┐ │
│  │ Emergent    │  │ Memory   │  │ Metrics      │ │
│  │ (涌现引擎)  │  │ (记忆)   │  │ (指标)       │ │
│  └────────────┘  └──────────┘  └─────────────┘ │
└─────────────────────────────────────────────────┘
```

**协作策略**: Consensus / Democratic / Bidding / Voting / Hybrid
**拓扑结构**: Star / Mesh / Ring / Tree / Dynamic
**关键特性**: Emergent Specialization / 共享记忆 / 性能指标 / 跨 Agent 通信

### 🌐 分布式调度器 (lingshu-distributed)

多节点部署基础设施。

```
┌─────────┐ ┌─────────┐ ┌──────────┐ ┌─────────┐
│ Cluster  │ │Scheduler│ │  Queue   │ │  Store  │
│ (集群)   │ │ (调度)  │ │ (队列)   │ │ (存储)  │
└────┬────┘ └────┬────┘ └────┬─────┘ └────┬────┘
     │           │           │            │
┌────▼────┐ ┌───▼────┐ ┌───▼──────┐
│ Leader  │ │ Cache  │ │ Gossip   │
│ (选举)   │ │ (缓存) │ │ (协议)   │
└─────────┘ └────────┘ └──────────┘
```

**调度策略**: RoundRobin / Adaptive / LoadBalance
**集群管理**: SWIM 故障检测 / Gossip 传播 / 领导者选举
**基础设施**: 分布式队列 / 缓存 / KV 存储 / 自动故障转移

### 🧠 自治 Runtime (lingshu-autonomy)

Agent 自我反思与自我进化能力。

```
┌─────────────────────────────────────────────┐
│              AutonomyEngine                    │
│  ┌──────────────┐  ┌──────────────────┐     │
│  │ Reflection    │  │ Evolution         │     │
│  │ Engine        │──│ Engine            │     │
│  │ (反思)        │  │ (进化)            │     │
│  └──────┬───────┘  └────────┬─────────┘     │
│         │                    │                │
│         ▼                    ▼                │
│  ┌──────────────────────────────────┐        │
│  │         ExperienceStore           │        │
│  │         (经验存储)                │        │
│  └──────────────────────────────────┘        │
└─────────────────────────────────────────────┘
```

**自我反思**: 经验分析 / 失败模式发现 / 性能退化检测 / 洞察生成
**自我进化**: 自动调参 / 行为优化 / 回滚保护 / 冷却机制

---

## 稳定性与质量

- ✅ cargo clippy --workspace -- -D warnings **零警告** (40+ crates)
- ✅ **193 个单元测试**全部通过
- ✅ **7 个 E2E 集成测试**（跨 crate 全链路验证）
- ✅ API 文档全面增强 (core / traits / distributed)
- ✅ **Criterion 基准测试套件**覆盖全模块
- ✅ **Gitee CI**: 6 个 Job 全自动验证

### CI Pipeline

| Job | 职责 |
|-----|------|
| `lint` | clippy + rustfmt + cargo doc |
| `test` | 单元测试 + doc 测试 + 集成测试 |
| `build` | Release 构建 + 冒烟测试 |
| `webui` | WASM 编译 |
| `docker` | 多架构 Docker 镜像 |
| `security` | cargo-audit + cargo-deny |

---

## 架构一览

```
lingshu/                     # 应用入口
├── core/                    # 核心类型 (LsId/LsError/LsContext)
├── traits/                  # 14 个公共接口 Trait
├── runtime/                 # 运行时 (v4.1 Production Runtime)
├── swarm/                   # 🆕 AgentSwarm 群体智能 (v5.0)
├── distributed/             # 🆕 分布式运行时 (v5.0)
├── autonomy/                # 🆕 Agent 自治 (v5.0)
├── evaluator/               # 评估框架
├── federation/              # 联邦协议
├── orchestrator/            # Agent 编排
├── backends/                # LLM 后端适配
├── webui/                   # WASM Web 控制台
├── mcp/                     # MCP 协议
├── memory/                  # 记忆存储
├── ... (40+ workspace crates)
```

---

## 基准测试基线 (aarch64-linux-android)

| 基准测试 | 时间 |
|----------|------|
| core/uuid_v4 | 1.06 µs |
| json/serialize_large | 1.14 µs |
| swarm/create_engine | 41.8 µs |
