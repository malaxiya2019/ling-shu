# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [5.0.0] - 2026-07-12

### Added — AgentSwarm 群体智能引擎 (v5.0.1)
- `lingshu-swarm` crate: AgentSwarm 群体智能协作框架
- 6 种群体决策策略: Voting / Consensus / Hierarchical / Democratic / Bidding / Hybrid
- 3 种通信通道: Broadcast、PointToPoint、Multicast
- 6 种专业化 Agent: Analyst / Creator / Validator / Negotiator / Observer / Tester
- 涌现专长引擎: 动态 Agent 专业分化 + 共享群体记忆
- 5 种动态拓扑: Star / Mesh / Ring / Tree / Dynamic（自适应切换）
- SwarmCoordinator: 任务分解 → 竞标 → Agent 选择 → 执行 → 评估 → 自适应
- MetricsCollector: P50/P90/P99 延迟指标
- 62 个单元测试, 覆盖率 100%

### Added — 分布式 Agent 调度器 (v5.0.2)
- `lingshu-distributed/scheduler.rs`: 跨节点 Agent 任务调度与负载均衡
- 6 种调度策略: LeastTasks / RoundRobin / Weighted / ConsistentHash / LocalFirst / Adaptive
- NodeLoad 综合负载评分系统 (pending/active/failure/CPU/memory)
- 自动健康检查心跳检测 + 超时节点清理
- 自动故障转移 (Auto-Failover) 检测失败节点
- DistributedQueue 集成（publish/consume/ack）
- 32 个单元测试, 覆盖率 100%

### Added — 自治 Runtime (v5.0.3)
- `lingshu-autonomy` crate: Agent 自我反思与自我进化引擎
- ExperienceStore: 7 种经验类型、5 级严重等级、自动裁剪、经验摘要统计
- ReflectionEngine: 检测重复失败模式、性能退化、效率机会、协作改进
- 8 种反思洞察类型, 优先级 + 置信度评分 + 改进建议
- 健康评分: 综合成功率、优先级惩罚、失败量加权计算
- EvolutionEngine: 10 种进化动作 (参数/策略/行为/重试/超时/验证/协作/资源/学习)
- 进化计划: 基于洞察自动生成 → 优先级排序 → 自动应用
- AgentParameters 可进化参数: temperature/tokens/timeout/retries/collaboration
- 进化冷却时间 + 自动回滚机制
- AutonomyEngine 顶层入口: 单步自治周期 (存储经验→反思→进化)
- 18 个单元测试, 覆盖率 100%

### Changed
- 在 `distributed/src/lib.rs` 注册 `scheduler` 模块并公开导出
- `Cargo.toml` workspace members 新增 `autonomy` crate
- 工作区 v5.0 三件套累计 112 个单元测试全部通过
