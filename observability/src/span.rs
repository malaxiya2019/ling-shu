//! Span 辅助工具.
//!
//! 提供基于 LsContext 的 span 创建函数，自动注入 `trace_id`、`session_id` 等字段。
//!
//! ## 用法
//! ```rust,ignore
//! use lingshu_observability::ls_span;
//!
//! let span = ls_span!("llm.invoke", ctx, model = %model_name);
//! let _guard = span.enter();
//! ```

use tracing::Span;

/// 创建 INFO 级别的 span，自动注入 LsContext 字段.
#[macro_export]
macro_rules! ls_span {
    ($name:expr, $ctx:expr $(, $key:ident = $val:expr)* $(,)?) => {{
        let ctx: &lingshu_core::LsContext = &$ctx;
        let span = tracing::span!(
            tracing::Level::INFO,
            $name,
            trace_id = %ctx.trace_id,
            session_id = %ctx.session_id,
            user_id = %ctx.user_id.as_deref().unwrap_or("-"),
            tenant_id = %ctx.tenant_id.as_deref().unwrap_or("-"),
            $($key = $val,)*
        );
        span
    }};
}

/// 创建 DEBUG 级别的 span.
#[macro_export]
macro_rules! ls_span_debug {
    ($name:expr, $ctx:expr $(, $key:ident = $val:expr)* $(,)?) => {{
        let ctx: &lingshu_core::LsContext = &$ctx;
        let span = tracing::span!(
            tracing::Level::DEBUG,
            $name,
            trace_id = %ctx.trace_id,
            session_id = %ctx.session_id,
            $($key = $val,)*
        );
        span
    }};
}

/// 创建 WARN 级别的 span.
#[macro_export]
macro_rules! ls_span_warn {
    ($name:expr, $ctx:expr $(, $key:ident = $val:expr)* $(,)?) => {{
        let ctx: &lingshu_core::LsContext = &$ctx;
        let span = tracing::span!(
            tracing::Level::WARN,
            $name,
            trace_id = %ctx.trace_id,
            session_id = %ctx.session_id,
            $($key = $val,)*
        );
        span
    }};
}

/// 在 span 作用域内执行同步函数并记录耗时.
#[inline]
pub fn instrument<T>(span: Span, f: impl FnOnce() -> T) -> T {
    let start = std::time::Instant::now();
    let _guard = span.enter();
    let result = f();
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    tracing::debug!(duration_ms, "completed");
    result
}

/// 在 span 作用域内执行异步函数并记录耗时.
#[inline]
pub async fn instrument_async<T>(
    span: Span,
    fut: impl std::future::Future<Output = T>,
) -> T {
    let start = std::time::Instant::now();
    let _guard = span.enter();
    let result = fut.await;
    drop(_guard);
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    tracing::debug!(duration_ms, "async completed");
    result
}

#[cfg(test)]
mod tests {
    use lingshu_core::{LsContext, LsId};
    use super::{instrument, instrument_async};
    

    #[test]
    fn test_ls_span_macro() {
        // 初始化 subscriber，忽略已设置的错误
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .try_init();
        let ctx = LsContext::with_session(LsId::new());
        let span = ls_span!("test.span", ctx);
        // 验证 span 能被正确创建
        let _guard = span.enter();
        // 在 span 内执行操作，验证不会 panic
        let _ = tracing::info_span!("nested");
    }

    #[test]
    fn test_ls_span_with_fields() {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .try_init();
        let ctx = LsContext::with_session(LsId::new());
        let span = ls_span!("test.fields", ctx, model = "gpt-4", tokens = 100u64);
        let _guard = span.enter();
        let _ = tracing::info_span!("nested");
    }

    #[test]
    fn test_instrument_sync() {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .try_init();
        let ctx = LsContext::with_session(LsId::new());
        let span = ls_span!("test.instrument", ctx);
        let result = instrument(span, || 42);
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_instrument_async() {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .try_init();
        let ctx = LsContext::with_session(LsId::new());
        let span = ls_span!("test.instrument_async", ctx);
        let result = instrument_async(span, async { 99 }).await;
        assert_eq!(result, 99);
    }

    #[test]
    fn test_ls_span_debug_macro() {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .try_init();
        let ctx = LsContext::with_session(LsId::new());
        let span = ls_span_debug!("test.debug", ctx, extra = "value");
        let _guard = span.enter();
    }

    #[test]
    fn test_ls_span_warn_macro() {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .try_init();
        let ctx = LsContext::with_session(LsId::new());
        let span = ls_span_warn!("test.warn", ctx);
        let _guard = span.enter();
    }
}
