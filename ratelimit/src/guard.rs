//! RateLimitGuard — 限流守卫，用于在 API 层或 Agent 执行前快速检查速率.
//!
//! 支持多层规则组合（例如：per-user 限流 + 全局限流）.

use crate::RateLimiter;
use lingshu_core::LsResult;

/// 限流决策.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitDecision {
    /// 通过.
    Allowed,
    /// 拒绝，包含配额信息.
    Denied {
        remaining: u64,
        reset_at: u64,
        limit: u64,
    },
}

/// 一条限流规则.
pub struct RateLimitRule {
    /// 规则名称（用于日志/调试）.
    pub name: String,
    /// 限流器实例.
    pub limiter: Box<dyn RateLimiter>,
    /// key 生成函数（从请求上下文中提取限流 key）.
    pub key_fn: Box<dyn Fn(&str) -> String + Send + Sync>,
}

impl std::fmt::Debug for RateLimitRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitRule")
            .field("name", &self.name)
            .finish()
    }
}

/// 限流守卫 — 组合多条规则进行检查.
pub struct RateLimitGuard {
    rules: Vec<RateLimitRule>,
}

impl RateLimitGuard {
    /// 创建空守卫.
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 添加规则.
    pub fn add_rule(&mut self, rule: RateLimitRule) {
        self.rules.push(rule);
    }

    /// 检查所有规则.
    ///
    /// `identity`: 用户或客户端标识.
    pub async fn check(&self, identity: &str) -> LsResult<RateLimitDecision> {
        for rule in &self.rules {
            let key = (rule.key_fn)(identity);
            let result = rule.limiter.check(&key).await?;
            if !result.allowed {
                return Ok(RateLimitDecision::Denied {
                    remaining: result.remaining,
                    reset_at: result.reset_at,
                    limit: result.limit,
                });
            }
        }
        Ok(RateLimitDecision::Allowed)
    }

    /// 仅查看，不消耗配额.
    pub async fn peek(&self, identity: &str) -> LsResult<RateLimitDecision> {
        for rule in &self.rules {
            let key = (rule.key_fn)(identity);
            let result = rule.limiter.peek(&key).await?;
            if !result.allowed {
                return Ok(RateLimitDecision::Denied {
                    remaining: result.remaining,
                    reset_at: result.reset_at,
                    limit: result.limit,
                });
            }
        }
        Ok(RateLimitDecision::Allowed)
    }
}

impl Default for RateLimitGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TokenBucket;

    #[tokio::test]
    async fn test_guard_single_rule() {
        let mut guard = RateLimitGuard::new();
        guard.add_rule(RateLimitRule {
            name: "per-user".into(),
            limiter: Box::new(TokenBucket::new(5, 1.0)),
            key_fn: Box::new(|id| format!("user:{id}")),
        });

        let decision = guard.check("alice").await.unwrap();
        assert_eq!(decision, RateLimitDecision::Allowed);
    }

    #[tokio::test]
    async fn test_guard_denies_when_exhausted() {
        let mut guard = RateLimitGuard::new();
        guard.add_rule(RateLimitRule {
            name: "per-user".into(),
            limiter: Box::new(TokenBucket::new(2, 1.0)),
            key_fn: Box::new(|id| format!("user:{id}")),
        });

        guard.check("alice").await.unwrap();
        guard.check("alice").await.unwrap();
        let decision = guard.check("alice").await.unwrap();
        assert!(matches!(
            decision,
            RateLimitDecision::Denied { remaining: 0, .. }
        ));
    }

    #[tokio::test]
    async fn test_guard_peek_does_not_consume() {
        let mut guard = RateLimitGuard::new();
        guard.add_rule(RateLimitRule {
            name: "per-user".into(),
            limiter: Box::new(TokenBucket::new(1, 1.0)),
            key_fn: Box::new(|id| format!("user:{id}")),
        });

        let decision = guard.peek("alice").await.unwrap();
        assert_eq!(decision, RateLimitDecision::Allowed);
        let decision = guard.check("alice").await.unwrap();
        assert_eq!(decision, RateLimitDecision::Allowed);
    }
}
