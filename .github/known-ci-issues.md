# 已知 CI 问题

## 上游依赖冲突

### starlark_map / hashbrown / allocative 版本冲突

**影响 Job：** build, test, feature-matrix (autoagents), feature-matrix (loong), feature-matrix (all features)

**错误信息：** `the trait bound RawTable<(K, V)>: Allocative is not satisfied`

**根因分析：**
- `starlark_map v0.13.0` 依赖 `hashbrown 0.14.5`，并对该版本的 `RawTable` 派生 `Allocative`
- `allocative v0.3.6` 仅对 `hashbrown 0.16.1` 实现了 `Allocative` trait
- 两个 `hashbrown` 版本不兼容，导致 `RawTable<(K, V)>: Allocative` 约束无法满足

**影响范围：** `autoagents` 和 `loong` 上游依赖引入的 `starlark_map`

**状态：** ⏳ 等待 `starlark_map` 上游更新 `allocative` 版本约束

**临时绕过：** 若不使用 `autoagents`/`loong` 功能，CI 基础检查全部通过

## 代码格式

`cargo fmt --all --check` 失败（约 1124 处差异）。在开发机上运行 `cargo fmt --all` 后提交即可修复。
