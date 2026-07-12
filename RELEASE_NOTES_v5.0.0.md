# 🎉 LingShu v5.0.0 — AgentSwarm + 分布式调度 + 自治 Runtime

**发布日期**: 2026-07-12  
**标签**: `v5.0.0`  
**仓库**: [https://gitee.com/liang2050/ling-shu](https://gitee.com/liang2050/ling-shu)

---

LingShu v5.0 是项目的一个重要里程碑。v4.x 系列完成了基础 Runtime、Memory、Tool、Workflow 和 MCP 能力的打磨，v5.0 在此基础上引入了 **Agent 群体协作**、**分布式调度** 和 **Agent 自治** 三大核心能力，使 LingShu 从一个单 Agent 框架升级为多 Agent 协作平台。

---

## 🚀 v5.0 三大新模块

### 🐝 AgentSwarm — 群体智能协作引擎 (`lingshu-swarm`)

多 Agent 协作框架，支持多种协作策略与动态拓扑切换。

| 特性 | 说明 |
|------|------|
| **协作策略** | Consensus / Democratic / Bidding / Voting / Hybrid |
| **拓扑结构** | Star / Mesh / Ring / Tree / Dynamic |
| **涌现专业化** | Agent 在协作中自动分化出专长角色 |
| **共享记忆** | Agent 之间共享上下文和知识 |
| **性能指标** | 实时监控吞吐量、延迟、成功率 |

### 🌐 分布式调度器 (`lingshu-distributed`)

多节点部署基础设施，支持集群管理和跨节点任务调度。

| 特性 | 说明 |
|------|------|
| **调度策略** | LeastTasks / RoundRobin / Weighted / ConsistentHash / LocalFirst / Adaptive |
| **集群管理** | SWIM 故障检测 + Gossip 传播 |
| **领导者选举** | Raft 风格租约机制 |
| **基础设施** | 分布式队列 / 缓存 / KV 存储 / 自动故障转移 |

### 🧠 自治 Runtime (`lingshu-autonomy`)

Agent 自我反思与自我进化能力，让 Agent 从经验中学习并自动优化。

| 特性 | 说明 |
|------|------|
| **经验存储** | 记录执行经验，支持标签和分类查询 |
| **自我反思** | 分析失败模式、检测性能退化、生成洞察 |
| **自我进化** | 自动调参、行为优化、回滚保护、冷却机制 |
| **完整自治周期** | 存储 → 反思 → 进化 → 验证 闭环 |

---

## ✅ 代码质量基线

| 检查项 | 状态 | 说明 |
|--------|------|------|
| `cargo clippy --workspace -D warnings` | ✅ 通过 | 40+ crate 零警告 |
| `cargo test --workspace` | ✅ 通过 | 193 个单元测试 |
| E2E 集成测试 | ✅ 通过 | 7 个跨 crate 全链路测试 |
| API 文档 | ✅ 增强 | core / traits / distributed 模块 |
| Criterion 基准测试 | ✅ 基线已建 | UUID / JSON / Swarm 性能基线 |
| Gitee CI | ✅ 已配置 | 6 个 Job 自动验证 |

---

## 📥 快速使用

```bash
# 1. 克隆仓库
git clone https://gitee.com/liang2050/ling-shu.git
cd ling-shu

# 2. 运行示例
cargo run --example swarm-basic        # Agent 群智能协作
cargo run --example distributed-basic  # 分布式集群调度
cargo run --example autonomy-basic     # 自我反思与进化
cargo run --example full-flow          # 三合一端到端集成

# 3. 运行全量测试
cargo test --workspace

# 4. 代码检查
cargo clippy --workspace -- -D warnings
```

---

## 🏗️ 架构总览

```
lingshu/
├── core/            核心类型 (LsId/LsError/LsContext)
├── traits/          14 个公共接口 Trait
├── runtime/         v4.1 Production Runtime
├── swarm/           🆕 AgentSwarm 群体智能 (v5.0)
├── distributed/     🆕 分布式运行时 (v5.0)
├── autonomy/        🆕 Agent 自治 (v5.0)
├── evaluator/      评估框架
├── federation/     联邦协议
├── orchestrator/    Agent 编排
├── backends/        LLM 后端适配
├── webui/           WASM Web 控制台
├── mcp/             MCP 协议
├── memory/          记忆存储
├── examples/        🆕 工程级示例
│   ├── swarm-basic/
│   ├── distributed-basic/
│   ├── autonomy-basic/
│   └── full-flow/
└── benches/         基准测试 (Criterion)
```

---

## 📊 基准测试基线

| 基准测试 | 耗时 (aarch64) |
|----------|---------------|
| core/uuid_v4 | 1.06 µs |
| json/serialize_large | 1.14 µs |
| swarm/create_engine | 41.8 µs |

---

## 🔜 后续路线

v5.x 系列将进入 **稳定阶段**：
- 修复 Bug、提升测试覆盖率
- 性能压测 Runtime / Memory / Swarm 瓶颈
- 完善 CLI、配置、文档和示例
- 支持更多 LLM、MCP Server、Tool 插件

v6.0 将规划多节点集群、跨机器 Agent 网络、更成熟的自治机制。

---

## 🙏 致谢

感谢所有贡献者和用户的支持。LingShu 从一个 Agent 运行时原型成长为包含 40+ crate、支持多 Agent 协作的完整平台，离不开社区的每一份贡献。

**让我们继续构建更智能的 Agent 生态！** 🚀
