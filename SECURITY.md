# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 3.x     | :white_check_mark: |
| < 3.0   | :x:                |

## Reporting a Vulnerability

如果发现安全漏洞, **请勿公开提交 Issue**。

请通过以下方式私下报告:

1. 发送邮件至项目维护者 (在 git commit log 中可找到)
2. 或在 GitHub 上创建 Security Advisory:
   https://github.com/malaxiya2019/ling-shu/security/advisories/new

我们会在 48 小时内确认收到, 并在修复后公开致谢。

## 安全最佳实践

### 生产部署

- 修改 `JWT_SECRET` 为 32+ 字符随机字符串
- 修改 `LINGSHU_CREDENTIAL_MASTER_KEY` 为强密码
- 启用 HTTPS (推荐通过反向代理如 nginx/caddy)
- 设置 `LS_ADMIN_PASSWORD` 为强密码
- 避免将 API Key 硬编码在配置文件或代码中

### API Key 安全

所有 LLM API Key 通过环境变量注入, 不落盘。

```bash
# 正确方式
export OPENAI_API_KEY="sk-..."
./start.sh --quick

# 或通过 .env 文件 (已在 .gitignore 中)
echo "OPENAI_API_KEY=sk-..." >> .env
```
