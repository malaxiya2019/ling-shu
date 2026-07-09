# Contributing to Lingshu

感谢您考虑为 Lingshu 贡献代码！以下是指南。

## 开发环境

```bash
git clone https://github.com/malaxiya2019/ling-shu.git
cd ling-shu
./start.sh --check-env   # 检查依赖
```

## 分支策略

- `main` — 稳定版本, 只接受 PR
- `develop` — 开发分支, 功能在此集成

## 提交 PR

1. Fork 仓库
2. 从 `develop` 创建功能分支: `git checkout -b feat/my-feature`
3. 提交前确保:
   ```bash
   cargo fmt --all
   cargo clippy --all-targets --all-features
   cargo test --all --all-features
   ```
4. 提交 PR 到 `develop` 分支

## 编码规范

- 遵循 Rust 标准命名规范 (snake_case, CamelCase)
- 所有公开 API 必须有 doc comment
- 错误类型使用 `LsError` 而非 `anyhow`/`Box<dyn Error>`
- 新增功能必须附带测试

## 项目结构

```
app/               # 主二进制入口
core/              # 核心类型
traits/            # 抽象接口
channel/           # 消息通道 (Telegram/飞书/QQ)
config/            # 配置系统
database/          # 数据库 (SQLite/PostgreSQL)
plugins/           # 内置插件
webui/             # Yew WASM 管理面板
```

## 获取帮助

- 提交 Issue: https://github.com/malaxiya2019/ling-shu/issues
- 发起 Discussion: https://github.com/malaxiya2019/ling-shu/discussions
