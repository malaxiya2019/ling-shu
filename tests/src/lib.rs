//! Lingshu 集成测试 — 跨 crate 端到端验证.

// 已有集成测试模块
#[cfg(test)]
pub(crate) mod evaluator_tests;

#[cfg(test)]
pub(crate) mod federation_tests;

#[cfg(test)]
pub(crate) mod evaluator_federation_e2e;

// Phase 1: 三大集成模块端到端测试
#[cfg(test)]
pub(crate) mod chidori_e2e;

#[cfg(test)]
pub(crate) mod autoagents_e2e;

#[cfg(test)]

// v5.0 跨 crate 端到端测试
#[cfg(test)]
pub(crate) mod swarm_autonomy_distributed_e2e;
#[cfg(test)]
pub(crate) mod loong_e2e;

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

    // ── 5. Mock Llm 多轮对话 ────────────────────────
    #[tokio::test]
    async fn test_mock_llm_multi_turn() {
        use lingshu_backends::mock_llm::MockLlm;
        use lingshu_core::{LsContext, LsId};
        use lingshu_traits::llm::*;

        let llm = MockLlm::new();
        let ctx = LsContext::with_session(LsId::new());
        for i in 0..3 {
            let msg = LlmMessage {
                role: LlmRole::User,
                content: format!("message {i}"),
                content_parts: None,
                name: None,
                tool_calls: None,
            };
            let request = LlmRequest {
                model: "mock".into(),
                messages: vec![msg],
                temperature: None,
                max_tokens: None,
                tools: None,
                stream: false,
            };
            let response = llm.invoke(ctx.child(), request).await.unwrap();
            assert!(!response.message.content.is_empty());
        }
    }

    // ── 6. ToolRegistry 注册与执行 ──────────────────
    #[tokio::test]
    async fn test_tool_registry() {
        use lingshu_runtime::ToolRegistry;

        let reg = ToolRegistry::new();
        assert_eq!(reg.count().await, 0);

        // Use a simple inline tool
        let tools = reg.list_tools().await;
        assert_eq!(tools.len(), 0);
    }

    // ── 7. SessionManager 生命周期 ──────────────────
    #[tokio::test]
    async fn test_session_manager() {
        use lingshu_core::{LsContext, LsId};
        use lingshu_runtime::session::{SessionManager, SessionState};

        let mgr = SessionManager::new(3600);
        let session_id = LsId::new();
        let ctx = LsContext::with_session(session_id);

        mgr.create(&ctx).await.unwrap();
        assert!(mgr.get(session_id).await.is_ok());
        assert_eq!(mgr.active_count().await, 1);

        mgr.terminate(session_id).await.unwrap();
        let info = mgr.get(session_id).await.unwrap();
        assert_eq!(info.state, SessionState::Terminated);

        assert!(mgr.get(session_id).await.is_ok());
        assert_eq!(mgr.active_count().await, 0);
    }

    // ── 8. Prompt AB Test ───────────────────────────
    #[tokio::test]
    async fn test_prompt_ab_test() {
        use lingshu_prompt::ABTestManager;
        use chrono::Utc;

        let mut manager = ABTestManager::new();
        manager
            .register(lingshu_prompt::ABTestConfig {
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

        let std = StdListener::bind("127.0.0.1:0").unwrap();
        let addr = std.local_addr().unwrap();
        let listener = tokio::net::TcpListener::from_std(std).unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service()).await.unwrap();
        });

        format!("http://{}", addr)
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
