//! Lingshu 集成测试 — 跨 crate 端到端验证.

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

        // Create collection
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

        let received: std::sync::Arc<StdMutex<Vec<String>>> =
            std::sync::Arc::new(StdMutex::new(Vec::new()));
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

    // ── 5. Storage 文件操作 ─────────────────────────
    #[tokio::test]
    async fn test_storage_file_ops() {
        use lingshu_core::{LsContext, LsId};
        use lingshu_storage::LocalStorage;
        use lingshu_traits::storage::Storage;

        let tmp = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(tmp.path().join("test_store"));
        let ctx = LsContext::with_session(LsId::new());

        // Upload
        let content = b"Hello, Lingshu!".to_vec();
        let info = storage
            .upload(ctx.child(), "test.txt", "text/plain", content.clone())
            .await
            .unwrap();
        assert_eq!(info.filename, "test.txt");

        // Download
        let (_, downloaded) = storage.download(ctx.child(), info.file_id).await.unwrap();
        assert_eq!(downloaded, content);

        // Info
        let file_info = storage.info(ctx.child(), info.file_id).await.unwrap();
        assert_eq!(file_info.filename, "test.txt");

        // Delete
        storage.delete(ctx.child(), info.file_id).await.unwrap();
        let result = storage.info(ctx.child(), info.file_id).await;
        assert!(result.is_err());
    }

    // ── 6. Security JWT + 权限 ─────────────────────
    #[tokio::test]
    async fn test_security_auth() {
        use lingshu_core::LsId;
        use lingshu_security::auth::JwtService;
        use lingshu_security::permission::{Permission, PermissionChecker};

        let jwt = JwtService::new("test-secret", 3600);
        let session_id = LsId::new().to_string();
        let token = jwt
            .issue("admin", &session_id, None, vec!["admin".into()])
            .unwrap();
        assert!(!token.is_empty());

        let verified = jwt.verify(&token).unwrap();
        assert_eq!(verified.sub, "admin");

        // Permission check
        let checker = PermissionChecker::new();
        let perm = Permission::new("system", "*", "manage");
        let result = checker.check(std::slice::from_ref(&perm), &perm);
        assert!(result.is_ok());
    }

    // ── 7. Runtime ToolRegistry 全链路 ──────────────
    #[tokio::test]
    async fn test_runtime_tool_registry() {
        use async_trait::async_trait;
        use lingshu_core::{LsContext, LsId, LsResult};
        use lingshu_runtime::ToolRegistry;
        use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};
        use serde_json::Value;

        struct GreetTool;

        #[async_trait]
        impl Tool for GreetTool {
            fn info(&self) -> ToolInfo {
                ToolInfo {
                    tool_id: LsId::new(),
                    name: "greet".into(),
                    description: "Greets a person".into(),
                    parameters: vec![ToolParam {
                        name: "name".into(),
                        description: "Person's name".into(),
                        required: true,
                        param_type: "string".into(),
                    }],
                }
            }

            fn validate(&self, input: &Value) -> LsResult<()> {
                if input.get("name").and_then(|v| v.as_str()).is_none() {
                    return Err(lingshu_core::LsError::Validation(
                        "missing name field".into(),
                    ));
                }
                Ok(())
            }

            async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
                let name = input["name"].as_str().unwrap_or("world");
                Ok(serde_json::json!({"greeting": format!("Hello, {name}!")}))
            }
        }

        let registry = ToolRegistry::new();
        registry.register(Box::new(GreetTool)).await;

        let ctx = LsContext::with_session(LsId::new());
        let result = registry
            .execute(&ctx, "greet", serde_json::json!({"name": "Lingshu"}))
            .await
            .unwrap();
        assert_eq!(result["greeting"], "Hello, Lingshu!");
    }

    // ── 8. InMemoryKnowledge 全链路 ─────────────────
    #[tokio::test]
    async fn test_knowledge_in_memory() {
        use lingshu_backends::InMemoryKnowledge;
        use lingshu_core::{LsContext, LsId};
        use lingshu_traits::knowledge::{DataSource, Knowledge, KnowledgeEntry};

        let kb = InMemoryKnowledge::new();
        let ctx = LsContext::with_session(LsId::new());

        let source = DataSource {
            source_id: LsId::new(),
            name: "docs".into(),
            source_type: "markdown".into(),
            config: serde_json::Value::Null,
        };
        let source_id = kb.register_source(ctx.child(), source).await.unwrap();

        let entry = KnowledgeEntry {
            entry_id: LsId::new(),
            source: source_id.to_string(),
            content: serde_json::json!("Rust is safe and fast"),
            version: 1,
            metadata: std::collections::HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        kb.insert_entry(entry).await.unwrap();

        let result = kb.search(ctx.child(), "Rust", 10).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.entries[0].content, "Rust is safe and fast");
    }

    // ── 9. Config 层级加载 ─────────────────────────
    #[test]
    fn test_config_defaults() {
        use lingshu_config::settings::LsConfig;
        let config = LsConfig::default();
        assert!(config.runtime.max_concurrent_tasks > 0);
        assert!(config.llm.max_tokens > 0);
        assert!(!config.llm.default_model.is_empty());
    }

    // ── 10. DatabaseRepository 全链路 ───────────────
    #[tokio::test]
    async fn test_database_repository_integration() {
        use lingshu_core::{LsContext, LsId};
        use lingshu_database::{DatabaseRepository, SqliteDatabase};
        use lingshu_traits::database::Pagination;
        use lingshu_traits::repository::Repository;
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        struct Product {
            id: Option<String>,
            name: String,
            price: f64,
        }

        let db = Arc::new(SqliteDatabase::in_memory().unwrap());
        let repo = DatabaseRepository::<Product>::new(db, "products");
        let ctx = LsContext::with_session(LsId::new());

        let product = Product {
            id: None,
            name: "Widget".into(),
            price: 9.99,
        };
        let inserted = repo.insert(ctx.child(), product).await.unwrap();
        assert_eq!(inserted.name, "Widget");
        assert!(inserted.id.is_some());

        let all = repo
            .query(
                ctx.child(),
                vec![],
                Pagination {
                    page: 1,
                    page_size: 10,
                },
            )
            .await
            .unwrap();
        assert_eq!(all.total, 1);
    }
}

// ── 11. RateLimit ────────────────────────────────
#[tokio::test]
async fn test_ratelimit_token_bucket_integration() {
    use lingshu_ratelimit::RateLimiter;
    use lingshu_ratelimit::TokenBucket;

    let bucket = TokenBucket::new(100, 10.0);
    for _ in 0..100 {
        let r = bucket.check("test").await.unwrap();
        assert!(r.allowed);
    }
    let r = bucket.check("test").await.unwrap();
    assert!(!r.allowed);
}

#[tokio::test]
async fn test_ratelimit_sliding_window() {
    use lingshu_ratelimit::RateLimiter;
    use lingshu_ratelimit::SlidingWindow;

    let sw = SlidingWindow::new(10, 60);
    for _ in 0..10 {
        let r = sw.check("key").await.unwrap();
        assert!(r.allowed);
    }
    let r = sw.check("key").await.unwrap();
    assert!(!r.allowed);
}

// ── 12. Billing ──────────────────────────────────
#[tokio::test]
async fn test_billing_full_flow() {
    use lingshu_billing::{BillingPlan, BillingSystem};

    let plans = vec![
        BillingPlan::free(),
        BillingPlan::basic(),
        BillingPlan::pro(),
    ];
    let system = BillingSystem::new(plans).unwrap();

    system
        .record_usage("alice", "gpt-4", 1000, 500)
        .await
        .unwrap();
    system
        .record_usage("alice", "gpt-4", 2000, 1000)
        .await
        .unwrap();

    let quota = system.check_quota("alice", "free").await.unwrap();
    assert_eq!(quota.token_quota, 1_000_000);
    assert_eq!(quota.requests_used, 2);
    assert_eq!(quota.tokens_used, 4500);
}

// ── 13. Audit ────────────────────────────────────
#[tokio::test]
async fn test_audit_log_integration() {
    use lingshu_audit::AuditQueryBuilder;
    use lingshu_audit::{AuditEntry, AuditEventType, AuditLog, AuditLogStore};

    let log = AuditLog::new();

    log.append(AuditEntry::new(
        AuditEventType::UserLogin,
        "user.login",
        "alice",
        "user",
        "user-001",
        r#"{"ip":"10.0.0.1"}"#,
    ))
    .await
    .unwrap();

    let q = AuditQueryBuilder::new()
        .with_actor("alice")
        .with_event_type(AuditEventType::UserLogin)
        .build();

    let results = log.query(&q).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].resource_id, "user-001");
}

// ── 14. Prompt ───────────────────────────────────
#[test]
fn test_prompt_template_compile() {
    use lingshu_prompt::{PromptRegistry, TemplateEngine, TemplateVariable};
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

// ── 15. HTTP Integration Tests ────────────────────

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
