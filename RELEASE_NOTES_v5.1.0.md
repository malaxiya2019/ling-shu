# 🎉 LingShu v5.1.0 — 工程质量加固 + CI 全线修复

**发布日期**: 2026-07-13  
**标签**: `v5.1.0`  
**仓库**: [https://github.com/malaxiya2019/ling-shu](https://github.com/malaxiya2019/ling-shu)

---

v5.0 引入了 **AgentSwarm 群体智能**、**分布式调度** 和 **自治 Runtime** 三大核心能力。  
v5.1 将重点放在 **工程质量加固** 上——修复 CI 基础设施、清理代码异味、消除弃用警告，使项目进入 **RC（Release Candidate）** 状态。

---

## 🛠️ CI 基础设施修复

### Git 依赖规范化
- **`chidori`**: 本地绝对路径 → Git 依赖（`github.com/malaxiya2019/chidori-fork`）
  - 解决了 CI 构建时路径不存在的问题
- **Tauri 系统库**: CI 镜像缺少 `glib-2.0` / `webkit2gtk-4.1` 等原生依赖
  - 在 `ci.yml` 和 `coverage.yml` 中添加 `libglib2.0-dev`、`libgtk-3-dev`、`libwebkit2gtk-4.1-dev`、`librsvg2-dev`、`patchelf`

### YAML 工作流修复
- **`ci.yml`**: `clippy lint` 步骤的 `run:` 字段因 YAML 缩进错误导致 workflow 解析失败
  - 修复后 CI 正常解析执行

### 弃用 API 更新
| 文件 | 问题 | 修复 |
|------|------|------|
| `runtime/src/chidori_recovery.rs` | `timestamp_nanos()` 已弃用 | 替换为 `timestamp_nanos_opt().unwrap_or(0)` |
| `federation/src/migration.rs` | `timestamp_nanos()` 已弃用 | 替换为 `timestamp_nanos_opt().unwrap_or(0)` |

### Clippy 警告清理
| 文件 | 问题 |
|------|------|
| `backends/src/workflow/sub_workflow.rs` | 未使用的导入 `WorkflowRegistryEntry` |
| `backends/src/workflow/sub_workflow.rs` | 未使用的变量 `handler` |

### Dead Code 清理
- **`security/src/oauth2.rs`**: 标记 `provider_metadata` 字段为 `#[allow(dead_code)]`（保留供未来使用，但不会被 `-Dwarnings` 拦截）

---

## ✅ 代码质量基线（v5.1）

| 检查项 | 结果 | 说明 |
|--------|:----:|------|
| `cargo clippy -D warnings` | ✅ 通过 | 40+ crate 零警告（含 audit-sqlite） |
| `cargo test --workspace` | ✅ 通过 | 单元测试全绿（核心 crate 500+ 测试通过） |
| `cargo check --no-default-features` | ✅ 通过 | 基础编译 |
| `cargo check --features chidori` | ✅ 通过 | chidori 集成 |
| `cargo check --features audit-sqlite` | ✅ 通过 | SQLite 审计日志持久化 |
| `cargo fuzz` (smoke) | ✅ 通过 | fuzz 测试无崩溃 |
| `cargo security` (audit + deny) | ✅ 通过 | 无已知漏洞 |
| `webui (WASM)` | ✅ 通过 | 前端编译成功（web-sys 特性格式修复） |
| `cargo fmt --all --check` | ⏳ CI 执行 | GitHub Actions lint job 自动检查 |

> **注**: `--all-features` 构建受上游 `starlark_map`/`hashbrown` 版本冲突影响（已记录至 `.github/known-ci-issues.md`），不影响核心功能。

## 📦 新增测试覆盖

自 v5.0.0 以来新增：

| 模块 | 新增测试数 | 内容 |
|------|:----------:|------|
| `lingshu-traits` | 89 | 完整测试套件 |
| `lingshu-swarm` | 新增 | 协作策略、拓扑切换、Agent 专业化 |
| `lingshu-distributed` | 新增 | 调度策略、故障转移 |
| `lingshu-autonomy` | 新增 | 反思循环、经验存储、进化回滚 |
| `lingshu-federation` | 新增 | 基准测试基线 |

---

## 📊 CI 健康状况

```
状态: RC → v5.1.0 Final（发布就绪）

LingShu 自身代码: ✅ 已稳定
审计日志 SQLite 持久化: ✅ 已集成
审计 API 模块化: ✅ 已完成（api/audit.rs）
CI 配置: ✅ 已更新（含 audit-sqlite 特性矩阵）
上游依赖:        ⏳ starlark_map/hashbrown 版本冲突（不影响核心功能）
发布准备程度:    ✅ 可发布
```

### 🔄 v5.1 RC 阶段新增内容

| 变更 | 文件 | 说明 |
|------|------|------|
| 审计 API 模块化提取 | `app/src/api/audit.rs` | 5 个 handler 从 full.rs 独立为 audit 模块 |
| SQLite 审计持久化 | `audit/src/sqlite.rs` | `SqliteAuditLog` 实现 `AuditLogStore` trait |
| app 运行时集成 | `app/src/main.rs` | feature-gated SQLite 审计初始化 |
| CI 特性矩阵 | `.github/workflows/ci.yml` | 新增 `audit-sqlite` 构建/测试/文档 |
| WebUI web-sys 修复 | `webui/Cargo.toml` | 修复特性数组格式 |
| Clippy 修复 | `audit/src/sqlite.rs`, `runtime/src/agent_pipeline.rs` | 3 项 clippy lint 修复 |
| 无用导入清理 | `swarm/src/lib.rs`, `tests/...e2e.rs` | 移除 unused imports |
| 测试结构修复 | `app/src/api/full.rs` | 为 LingshuRuntime 测试补充新字段初始化 |

## 🔜 后续规划

| 事项 | 优先级 | 说明 |
|------|:------:|------|
| `cargo fmt --all` | 中 | CI 自动执行（GitHub Actions lint job） |
| 文档完善 | 中 | README 快速开始、配置说明 |
| 示例丰富 | 中 | 增加更多使用场景示例 |
| crates.io 发布 | 低 | 如需公开发包 |
| 上游 `starlark_map` 修复 | 被动 | 等待上游更新后 CI 自动恢复 |

## 🙏 致谢

感谢所有贡献者和用户的反馈。v5.1 虽然没有引入新功能，但工程质量的提升是项目长期健康的基础。

**欢迎提交 Issue 或 PR！** 🚀
