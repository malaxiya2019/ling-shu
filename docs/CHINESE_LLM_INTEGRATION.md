# 🇨🇳 lingshu 国内大模型集成指南

> 2026-07-09: 已集成 DeepSeek、阿里千问 Qwen、智谱 GLM、百度文心 ERNIE

## 快速开始

### 1. 环境变量配置

选择以下任一提供商，设置对应环境变量即可：

```bash
# ── DeepSeek ──
export LLM_PROVIDER=deepseek
export DEEPSEEK_API_KEY=sk-your-deepseek-api-key
# 模型: deepseek-chat, deepseek-reasoner

# ── 阿里千问 Qwen ──
export LLM_PROVIDER=qwen
export QWEN_API_KEY=sk-your-qwen-api-key
# 或 QWEN_API_KEY 的别名
export DASHSCOPE_API_KEY=sk-your-dashscope-key
# 模型: qwen-plus, qwen-turbo, qwen-max, qwen-long

# ── 智谱 GLM ──
export LLM_PROVIDER=zhipu
export ZHIPU_API_KEY=your-zhipu-api-key
# 模型: glm-4-plus, glm-4-flash, glm-4-air

# ── 百度文心 ERNIE ──
export LLM_PROVIDER=baidu
export BAIDU_API_KEY=your-baidu-api-key
# 模型: ernie-4.0, ernie-3.5, ernie-speed

# ── 也可以配置默认模型 ──
export LS_LLM_DEFAULT_MODEL=deepseek-chat
```

### 2. 使用 REPL 测试

```bash
lingshu --repl

# 在 REPL 中输入
你好，请介绍一下你自己
```

### 3. YAML 配置文件

`config/dev.yaml`:
```yaml
llm:
  provider: deepseek
  default_model: deepseek-chat
  max_tokens: 4096
  timeout_seconds: 120
```

## 架构说明

所有国内模型均通过 **OpenAI 兼容接口** 统一接入：

| 提供商 | 兼容端点 | 认证方式 |
|--------|---------|---------|
| DeepSeek | `https://api.deepseek.com/v1` | API Key |
| 阿里千问 | `https://dashscope.aliyuncs.com/compatible-mode/v1` | API Key |
| 智谱 GLM | `https://open.bigmodel.cn/api/paas/v4` | API Key |
| 百度文心 | `https://qianfan.baidubce.com/v2` | API Key |

代码实现位于 `backends/src/llm_factory.rs`，每个 provider 通过 `OpenAiLlm::new(key, model, Some(base_url))` 构建。

## 推荐模型

| 场景 | 推荐模型 | 性价比 |
|------|---------|--------|
| 日常对话 | DeepSeek-chat 或 Qwen-plus | ⭐⭐⭐ 极高 |
| 推理/编程 | DeepSeek-reasoner | ⭐⭐⭐ 极高 |
| 长文本 | Qwen-long | ⭐⭐ 高 |
| 快速响应 | GLM-4-flash | ⭐⭐⭐ 极高 |
| 中文优化 | ERNIE-4.0 | ⭐⭐ 高 |
