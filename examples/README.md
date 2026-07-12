# LingShu 示例集 (v5.0)

本目录包含 LingShu v5.0 的完整示例，演示三大核心模块的使用方式。

## 示例列表

| 示例 | 说明 | 涉及模块 |
|------|------|----------|
| [`swarm-basic`](./swarm-basic/) | Agent 群智能协作 | Swarm |
| [`distributed-basic`](./distributed-basic/) | 分布式集群调度 | Distributed |
| [`autonomy-basic`](./autonomy-basic/) | 自我反思与进化 | Autonomy |
| [`full-flow`](./full-flow/) | 三合一端到端集成 | Swarm + Distributed + Autonomy |

## 运行方式

```bash
# Swarm 基础示例
cargo run --example swarm-basic

# 分布式调度示例
cargo run --example distributed-basic

# 自治引擎示例
cargo run --example autonomy-basic

# 全流程集成示例（推荐）
cargo run --example full-flow
```

> **注意**：所有示例均在 `#[tokio::main]` 异步运行时中运行，需要 tokio 支持。
> 部分示例（如 `distributed-basic`）需要集群模拟，会在本地创建伪节点。

## 依赖关系

```
ling-shu/
├── swarm-basic/       → lingshu-swarm + lingshu-core
├── distributed-basic/ → lingshu-distributed + lingshu-core
├── autonomy-basic/    → lingshu-autonomy + lingshu-core
└── full-flow/         → 全部三个模块
```

## 推荐学习路径

1. **swarm-basic** — 先了解 Agent 如何协作
2. **distributed-basic** — 再了解任务如何跨节点调度
3. **autonomy-basic** — 然后了解 Agent 如何自我进化
4. **full-flow** — 最后体验三者如何协同工作
