//! Lingshu 集成测试 — 跨 crate 端到端验证.

// 端到端测试模块（evaluator + federation）
#[cfg(test)]
pub(crate) mod evaluator_tests;

#[cfg(test)]
pub(crate) mod federation_tests;

#[cfg(test)]
pub(crate) mod evaluator_federation_e2e;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    // ── 1. Core 基础类型 ────────────────────────────
    #[test]
    fn test_core_id_serialization() {
        use lingshu_core::LsId;
        let id = LsId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: LsId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_core_error_roundtrip() {
        use lingshu_core::LsError;
        let err = LsError::NotFound("test".into());
        let json = serde_json::to_string(&err).unwrap();
        let deserialized: LsError = serde_json::from_str(&json).unwrap();
        assert!(deserialized.to_string().contains("not found"));
    }

    #[test]
    fn test_core_context_with_metadata() {
        use lingshu_core::{LsContext, LsId};
        let ctx = LsContext::with_session(LsId::new())
            .with_user("test-user")
            .with_metadata("key", "value");
        assert_eq!(ctx.user_id, Some("test-user".to_string()));
        let val = ctx.metadata.get("key").map(|s| s.as_str());
        assert_eq!(val, Some("value"));
    }

    // ── 2. LLM Mock 全链路 ──────────────────────────
    #[tokio::test]
    async fn test_llm_mock_chat() {
        use lingshu_backends::mock_llm::MockLlm;
        use lingshu_core::{LsContext, LsId};
        use lingshu_traits::llm::*;

        let llm = MockLlm::new();
        let ctx = LsContext::with_session(LsId::new());
        let msg = LlmMessage {
            role: LlmRole::User,
            content: "Hello".into(),
            content_parts: None,
            name: None,
            tool_calls: None,
        };
        let request = LlmRequest {
            model: "mock".into(),
            messages: vec![msg],
            temperature: Some(0.7),
            max_tokens: Some(100),
            tools: None,
            stream: false,
        };
        let response = llm.invoke(ctx, request).await.unwrap();
        assert!(!response.message.content.is_empty());
        assert!(matches!(response.message.role, LlmRole::Assistant));
    }

    // ── 3. InMemoryVectorStore 全链路 ───────────────
    #[tokio::test]
    async fn test_vector_store_crud() {
        use lingshu_backends::InMemoryVectorStore;
        use lingshu_core::{LsContext, LsId};
        use lingshu_traits::vector_store::{VectorRecord, VectorStore};

        let store = InMemoryVectorStore::new();
        let ctx = LsContext::with_session(LsId::new());

        let coll_id = store
            .create_collection(ctx.child(), "test", 3)
            .await
            .unwrap();

        let record = VectorRecord {
            id: LsId::new(),
            vector: vec![0.1, 0.2, 0.3],
            metadata: serde_json::json!({"text": "hello"}),
            score: None,
        };
        store
            .upsert(ctx.child(), coll_id, vec![record.clone()])
            .await
            .unwrap();

        let results = store
            .search(ctx, coll_id, vec![0.1, 0.2, 0.3], 5)
            .await
            .unwrap();
        assert_eq!(results.records.len(), 1);
        assert_eq!(results.records[0].metadata["text"], "hello");
    }

    // ── 4. EventBus 发布订阅 ────────────────────────
    #[tokio::test]
    async fn test_eventbus_publish_subscribe() {
        use lingshu_core::{LsContext, LsId};
        use lingshu_eventbus::bus::InMemoryEventBus;
        use lingshu_traits::event_bus::{Event, EventBus};
        use serde_json::json;
        use std::sync::Mutex as StdMutex;

        let bus = InMemoryEventBus::new();
        let ctx = LsContext::with_session(LsId::new());

        let received: Arc<StdMutex<Vec<String>>> =
            Arc::new(StdMutex::new(Vec::new()));
        let rx = received.clone();

        bus.subscribe(
            ctx.child(),
            "test.*",
            Box::new(move |evt: Event| {
                rx.lock().unwrap().push(evt.topic);
                Ok(())
            }),
        )
        .await
        .unwrap();

        let event = Event {
            event_id: "evt_001".into(),
            topic: "test.event".into(),
            session_id: LsId::new().to_string(),
            trace_id: LsId::new().to_string(),
            payload: json!({"data": 42}),
            timestamp: chrono::Utc::now(),
        };
        bus.publish(ctx.child(), event).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let data = received.lock().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], "test.event");
    }

    // ── 5. Plugin EventBus ─────────────────────────
    #[tokio::test]
    async fn test_plugin_event_bus() {
        use lingshu_plugin::event::{Event, EventBus, EventType};
        use std::sync::atomic::{AtomicUsize, Ordering};

        let bus = Arc::new(EventBus::new());
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        let registrar = bus.registrar();
        registrar
            .register(
                EventType::PluginInstalled,
                Arc::new(move |_evt: Event| {
                    c.fetch_add(1, Ordering::SeqCst);
                }),
                "test handler",
            )
            .await;

        let event = Event::new(EventType::PluginInstalled, "test", serde_json::json!({"key": "value"}));
        bus.publish(&event).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // ── 6. Plugin 清单解析 ──────────────────────
    #[tokio::test]
    async fn test_plugin_manifest_parsing() {
        use lingshu_plugin::manifest::parse_manifest;

        let json = serde_json::json!({
            "name": "test-plugin",
            "version": "1.0.0",
            "description": "Test plugin",
            "author": "test",
            "license": "MIT",
            "entry": "test.wasm",
            "permissions": []
        });

        let manifest = parse_manifest(&json.to_string()).expect("parse manifest");
        assert_eq!(manifest.base.name, "test-plugin");
        assert_eq!(manifest.base.version, "1.0.0");
    }

    // ── 7. Rate Limiter (TokenBucket) ──────────────
    #[tokio::test]
    async fn test_rate_limiter_bucket() {
        use lingshu_ratelimit::bucket::TokenBucket;
        use lingshu_ratelimit::RateLimiter;

        let bucket = TokenBucket::new(5, 10.0); // 容量 5, 每秒恢复 10

        // 测试 5 次内通过
        for i in 0..5 {
            let result = bucket.check("test-key").await.unwrap();
            assert!(
                result.allowed,
                "request {} should be allowed",
                i + 1
            );
        }

        // 第 6 次应当限流
        let result = bucket.check("test-key").await.unwrap();
        assert!(!result.allowed, "6th request should be denied");
    }

    // ── 8. Prompt Template ─────────────────────────
    #[test]
    fn test_prompt_template_compile() {
        use lingshu_prompt::registry::PromptRegistry;
        use lingshu_prompt::{TemplateEngine, TemplateVariable};
        use std::collections::HashMap;

        let engine = TemplateEngine::new();
        let registry = PromptRegistry::new();

        registry
            .register(
                "test-prompt",
                "Integration test prompt",
                "Answer in {{ language }}: {{ query }}",
                vec![
                    TemplateVariable {
                        name: "language".into(),
                        description: Some("Output language".into()),
                        required: true,
                        default_value: None,
                    },
                    TemplateVariable {
                        name: "query".into(),
                        description: Some("User query".into()),
                        required: true,
                        default_value: None,
                    },
                ],
            )
            .unwrap();

        let mut vars = HashMap::new();
        vars.insert("language".to_string(), "Chinese".to_string());
        vars.insert("query".to_string(), "What is Rust?".to_string());

        let compiled = registry.compile("test-prompt", &vars, &engine).unwrap();
        assert_eq!(compiled.text, "Answer in Chinese: What is Rust?");
    }

    #[tokio::test]
    async fn test_prompt_ab_test() {
        use chrono::Utc;
        use lingshu_prompt::ABTestConfig;
        use lingshu_prompt::ABTestManager;

        let mut manager = ABTestManager::new();
        manager
            .register(ABTestConfig {
                name: "test-ab".into(),
                variant_a: "v1".into(),
                variant_b: "v2".into(),
                traffic_percent_b: 50,
                enabled: true,
                start_at: Utc::now(),
                end_at: None,
            })
            .unwrap();

        manager.record_result("test-ab", "v1", true, 100.0).unwrap();
        manager.record_result("test-ab", "v2", true, 80.0).unwrap();

        let result = manager.get_result("test-ab").unwrap();
        assert_eq!(result.a_count, 1);
        assert_eq!(result.b_count, 1);
    }

    // ── 9. Websocket Connection Manager ────────────
    #[tokio::test]
    async fn test_websocket_connection_manager() {
        use lingshu_websocket::connection::{Connection, ConnectionManager};
        #[allow(unused_imports)] use lingshu_websocket::types::ConnectionState;

        let mgr = ConnectionManager::new(300);

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let conn = Connection::new(
            "session-1".to_string(),
            "user-1".to_string(),
            tx,
            "127.0.0.1:9999".to_string(),
            "test-agent".to_string(),
        );
        mgr.register(conn).await;

        let found = mgr.get("session-1").await;
        assert!(found.is_some(), "connection should be registered");

        let count = mgr.active_count().await;
        assert_eq!(count, 1, "should have 1 active connection");

        mgr.unregister("session-1").await;
        let count = mgr.active_count().await;
        assert_eq!(count, 0, "should have 0 active connections");
    }

    // ── 10. HTTP Integration Tests ─────────────────

    /// 启动一个最小的测试 HTTP 服务器，返回 base URL。
    #[cfg(test)]
    pub(crate) async fn spawn_test_server() -> String {
        use axum::{routing::get, Json, Router};
        use std::net::TcpListener as StdListener;

        let app = Router::new()
            .route("/health", get(|| async {
                Json(serde_json::json!({
                    "status": "ok",
                    "version": "1.0.0",
                    "uptime": "integration-test",
                    "checks": []
                }))
            }))
            .route("/version", get(|| async {
                Json(serde_json::json!({
                    "version": "1.0.0",
                    "build": "integration-test",
                    "rustc": "stable"
                }))
            }))
            .route("/v1/models", get(|| async {
                Json(serde_json::json!([
                    {"id": "gpt-4o", "object": "model", "created": 1700000000, "owned_by": "openai"},
                    {"id": "claude-3-opus", "object": "model", "created": 1700000000, "owned_by": "anthropic"},
                    {"id": "mock-model", "object": "model", "created": 1700000000, "owned_by": "lingshu"}
                ]))
            }));

        let listener = StdListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let tcp_listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let bound_addr = tcp_listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(tcp_listener, app).await.unwrap();
        });

        format!("http://{}", bound_addr)
    }

    #[tokio::test]
    async fn int_test_health_endpoint() {
        let base_url = spawn_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .get(format!("{}/health", base_url))
            .send()
            .await
            .expect("health request failed");

        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], "1.0.0");
    }

    #[tokio::test]
    async fn int_test_version_endpoint() {
        let base_url = spawn_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .get(format!("{}/version", base_url))
            .send()
            .await
            .expect("version request failed");

        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["version"], "1.0.0");
    }

    #[tokio::test]
    async fn int_test_models_endpoint() {
        let base_url = spawn_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .get(format!("{}/v1/models", base_url))
            .send()
            .await
            .expect("models request failed");

        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let models: Vec<serde_json::Value> = resp.json().await.unwrap();
        assert!(models.len() >= 3);
        assert!(models.iter().any(|m| m["id"] == "gpt-4o"));
    }
}
