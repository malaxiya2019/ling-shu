use serde::{Deserialize, Serialize};
use thiserror::Error;

/// LsError — 统一错误类型，全系统唯一错误表示.
#[derive(Debug, Clone, Serialize, Deserialize, Error)]
#[non_exhaustive]
pub enum LsError {
    // ── 通用 ──────────────────────────────────────────
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("timeout: {0}")]
    Timeout(String),

    // ── Runtime ────────────────────────────────────────
    #[error("runtime not initialized")]
    RuntimeNotInitialized,

    #[error("runtime already initialized")]
    RuntimeAlreadyInitialized,

    #[error("runtime state error: {0}")]
    RuntimeState(String),

    // ── Session ────────────────────────────────────────
    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("session expired: {0}")]
    SessionExpired(String),

    // ── Permission & Security ──────────────────────────
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),

    // ── Plugin ─────────────────────────────────────────
    #[error("plugin error: {0}")]
    Plugin(String),

    #[error("plugin not found: {0}")]
    PluginNotFound(String),

    // ── LLM / Embedding ────────────────────────────────
    #[error("llm error: {0}")]
    Llm(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    // ── Storage ────────────────────────────────────────
    #[error("storage error: {0}")]
    Storage(String),

    // ── EventBus ───────────────────────────────────────
    #[error("eventbus error: {0}")]
    EventBus(String),

    // ── Config ─────────────────────────────────────────
    #[error("config error: {0}")]
    Config(String),

    // ── Serialization ──────────────────────────────────
    #[error("serialization error: {0}")]
    Serialization(String),

    // ── External / Unknown ─────────────────────────────
    #[error("external error: {0}")]
    External(String),
}

/// LsResult — 统一返回类型.
pub type LsResult<T> = Result<T, LsError>;

impl From<serde_json::Error> for LsError {
    fn from(e: serde_json::Error) -> Self {
        LsError::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = LsError::PermissionDenied("read secret".into());
        assert!(err.to_string().contains("permission denied"));
    }

    #[test]
    fn test_result_alias() {
        fn works() -> LsResult<i32> {
            Ok(42)
        }
        assert_eq!(works().unwrap(), 42);
    }
}
