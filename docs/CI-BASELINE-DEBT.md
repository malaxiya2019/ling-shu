# CI Baseline Debt

跟踪 main 分支上已有的 CI 失败，与 RFC-0052 无关。

## Status

- [x] RFC-0052 Terminal Runtime Isolation merged (v5.2.0)
- [ ] fix loong dependency conflict (`feature-matrix (loong)` fails)
- [ ] repair existing tests (`cargo test --all` fails)
- [ ] docker CI stabilization (`Docker Build & Publish` fails)
- [ ] clippy-strict warnings cleanup (optional, quality gate)

## Context

These failures are pre-existing on `main` and were not introduced by RFC-0052.
See merge commit `d462283` for details.

## Affected Workflows

| Workflow | Failure | Root Cause |
|---|---|---|
| `feature-matrix (loong)` | compilation error | loong crate dependency |
| `feature-matrix (chidori,autoagents,loong)` | compilation error | same loong issue |
| `test` | test compilation error | pre-existing test issue |
| `Docker Build & Publish` | buildx failure | docker environment issue |
| `clippy-strict` | zero-warning violation | workspace-wide pre-existing warnings (by design) |

## Resolution

- [ ] Fix loong crate compatibility
- [ ] Fix existing test compilation
- [ ] Stabilize Docker CI build
- [ ] Clean up clippy warnings for strict gate
