//! 测试所有 14 个 Trait 的数据结构：序列化/反序列化、字段验证、边界条件.

use lingshu_core::LsId;
use lingshu_traits::*;
use serde_json::json;

// ── 1. Agent ────────────────────────────────────────

#[test]
fn test_agent_status_roundtrip() {
    for status in &[agent::AgentStatus::Idle, agent::AgentStatus::Running, agent::AgentStatus::Completed] {
        let json = serde_json::to_string(status).unwrap();
        let back: agent::AgentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(*status, back);
    }
}

#[test]
fn test_agent_snapshot_creation() {
    let snap = agent::AgentSnapshot {
        agent_id: LsId::new(),
        status: agent::AgentStatus::Running,
        context: lingshu_core::LsContext::with_session(LsId::new()),
        state: vec![1, 2, 3],
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_value(&snap).unwrap();
    assert!(json.get("agent_id").is_some());
    assert!(json.get("state").is_some());
}

#[test]
fn test_agent_output_success() {
    let out = agent::AgentOutput {
        agent_id: LsId::new(),
        status: agent::AgentStatus::Completed,
        data: Some(json!({"result": "ok"})),
        error: None,
    };
    let json = serde_json::to_value(&out).unwrap();
    assert_eq!(json["data"]["result"], "ok");
    assert!(json["error"].is_null());
}

// ── 2. Runtime ──────────────────────────────────────

#[test]
fn test_runtime_status_ordering() {
    use runtime::RuntimeStatus;
    assert_ne!(RuntimeStatus::Uninitialized, RuntimeStatus::Running);
    assert_eq!(
        serde_json::from_str::<RuntimeStatus>("\"Running\"").unwrap(),
        RuntimeStatus::Running
    );
}

#[test]
fn test_runtime_stats_defaults() {
    let stats = runtime::RuntimeStats {
        uptime_seconds: 0,
        active_sessions: 0,
        active_tasks: 0,
        total_tasks_completed: 0,
        total_tasks_failed: 0,
    };
    let json = serde_json::to_value(&stats).unwrap();
    assert_eq!(json["uptime_seconds"], 0);
}

// ── 3. Scheduler ────────────────────────────────────

#[test]
fn test_priority_ordering() {
    use scheduler::Priority;
    assert!(Priority::Critical as u8 > Priority::Normal as u8);
    assert_eq!(serde_json::from_str::<scheduler::Priority>("\"High\"").unwrap(), Priority::High);
}

#[test]
fn test_task_info_serialization() {
    use scheduler::{Priority, TaskInfo, TaskStatus};
    let info = TaskInfo {
        task_id: LsId::new(),
        session_id: LsId::new(),
        priority: Priority::High,
        status: TaskStatus::Running,
        created_at: chrono::Utc::now(),
        tags: [("env".into(), "test".into())].into(),
    };
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["priority"], "High");
    assert_eq!(json["tags"]["env"], "test");
}

// ── 4. Plugin ───────────────────────────────────────

#[test]
fn test_plugin_info_default_perms() {
    let manifest = plugin::PluginManifest {
        name: "test".into(),
        version: "1.0.0".into(),
        description: "test plugin".into(),
        permissions: vec![],
        ..Default::default()
    };
    let info = plugin::PluginInfo {
        plugin_id: LsId::new(),
        manifest,
        status: plugin::PluginStatus::Installed,
        loaded_at: None,
    };
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["manifest"]["name"], "test");
    assert!(json["manifest"]["permissions"].as_array().unwrap().is_empty());
}

#[test]
fn test_plugin_permission_format() {
    let perm = plugin::PluginPermission {
        resource: "ls.agent.run".into(),
        actions: vec!["execute".into(), "pause".into()],
    };
    let json = serde_json::to_value(&perm).unwrap();
    assert_eq!(json["actions"][0], "execute");
}

// ── 5. Tool ─────────────────────────────────────────

#[test]
fn test_tool_param_required() {
    use tool::ToolParam;
    let param = ToolParam {
        name: "query".into(),
        description: "search term".into(),
        required: true,
        param_type: "string".into(),
    };
    assert!(param.required);
    let back: ToolParam = serde_json::from_value(serde_json::to_value(&param).unwrap()).unwrap();
    assert_eq!(back.name, "query");
}

#[test]
fn test_tool_call_record() {
    use tool::ToolCallRecord;
    let rec = ToolCallRecord {
        tool_id: LsId::new(),
        call_id: LsId::new(),
        session_id: LsId::new(),
        input: json!({"x": 1}),
        output: json!({"y": 2}),
        duration_ms: 42,
        success: true,
        timestamp: chrono::Utc::now(),
    };
    assert!(rec.success);
    assert_eq!(rec.duration_ms, 42);
}

// ── 6. Memory ───────────────────────────────────────

#[test]
fn test_memory_item_with_ttl() {
    use memory::MemoryItem;
    let item = MemoryItem {
        memory_id: LsId::new(),
        session_id: LsId::new(),
        content: json!("hello"),
        metadata: [("source".into(), "test".into())].into(),
        created_at: chrono::Utc::now(),
        ttl_seconds: Some(3600),
    };
    assert_eq!(item.ttl_seconds, Some(3600));
    let json = serde_json::to_value(&item).unwrap();
    assert_eq!(json["ttl_seconds"], 3600);
}

// ── 7. Knowledge ────────────────────────────────────

#[test]
fn test_knowledge_entry_versioning() {
    use knowledge::KnowledgeEntry;
    let entry = KnowledgeEntry {
        entry_id: LsId::new(),
        source: "wiki".into(),
        content: json!("data"),
        version: 3,
        metadata: Default::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert_eq!(entry.version, 3);
    let back: KnowledgeEntry = serde_json::from_value(serde_json::to_value(&entry).unwrap()).unwrap();
    assert_eq!(back.version, 3);
    assert_eq!(back.source, "wiki");
}

// ── 8. Llm ──────────────────────────────────────────

#[test]
fn test_llm_messages() {
    use llm::{LlmMessage, LlmRole};
    let msgs = vec![
        LlmMessage { role: LlmRole::System, content: "You are a helper.".into(), name: None, tool_calls: None },
        LlmMessage { role: LlmRole::User, content: "Hello!".into(), name: None, tool_calls: None },
    ];
    let json = serde_json::to_value(&msgs).unwrap();
    assert_eq!(json[0]["role"], "System");
    assert_eq!(json[1]["content"], "Hello!");
}

#[test]
fn test_llm_usage_zero() {
    use llm::LlmUsage;
    let u = LlmUsage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 };
    let json = serde_json::to_value(&u).unwrap();
    assert_eq!(json["total_tokens"], 0);
}

// ── 9. Embedding ────────────────────────────────────

#[test]
fn test_embedding_vector_dimensions() {
    use embedding::EmbeddingVector;
    let v = EmbeddingVector { dimensions: 768, values: vec![0.1; 768] };
    assert_eq!(v.values.len(), v.dimensions);
    let back: EmbeddingVector = serde_json::from_value(serde_json::to_value(&v).unwrap()).unwrap();
    assert_eq!(back.dimensions, 768);
}

#[test]
fn test_embedding_request_batch() {
    use embedding::EmbeddingRequest;
    let req = EmbeddingRequest { input: vec!["a".into(), "b".into()], model: Some("text-embedding-3-small".into()) };
    assert_eq!(req.input.len(), 2);
}

// ── 10. VectorStore ─────────────────────────────────

#[test]
fn test_vector_record_score() {
    use vector_store::VectorRecord;
    let rec = VectorRecord {
        id: LsId::new(),
        vector: vec![0.1, 0.2, 0.3],
        metadata: json!({"doc": "test"}),
        score: Some(0.95),
    };
    assert_eq!(rec.score, Some(0.95));
    let back: VectorRecord = serde_json::from_value(serde_json::to_value(&rec).unwrap()).unwrap();
    assert_eq!(back.score, Some(0.95_f64));
}

// ── 11. Database ────────────────────────────────────

#[test]
fn test_pagination_calculation() {
    use database::{PaginatedResult, Pagination};
    let p = Pagination { page: 1, page_size: 10 };
    let res = PaginatedResult {
        items: vec![json!("a"), json!("b")],
        total: 2,
        page: p.page,
        page_size: p.page_size,
        total_pages: 1,
    };
    assert_eq!(res.total_pages, 1);
}

#[test]
fn test_query_filter_operators() {
    use database::QueryFilter;
    let f = QueryFilter { field: "age".into(), operator: "gte".into(), value: json!(18) };
    assert_eq!(f.operator, "gte");
}

// ── 12. EventBus ────────────────────────────────────

#[test]
fn test_event_required_fields() {
    use event_bus::Event;
    let ev = Event {
        event_id: "evt_001".into(),
        topic: "ls.agent.run.completed".into(),
        session_id: "sess_001".into(),
        trace_id: "trace_001".into(),
        payload: json!({"status": "ok"}),
        timestamp: chrono::Utc::now(),
    };
    assert!(ev.event_id.starts_with("evt"));
    assert_eq!(ev.topic.split('.').collect::<Vec<_>>()[0], "ls");
}

// ── 13. Repository ──────────────────────────────────

#[test]
fn test_database_repository_new() {
    use repository::DatabaseRepository;
    let repo = DatabaseRepository::<String>::new("users");
    assert_eq!(repo.collection_name(), "users");
}

// ── 14. Storage ─────────────────────────────────────

#[test]
fn test_file_info_minimal() {
    use storage::FileInfo;
    let info = storage::FileInfo {
        file_id: LsId::new(),
        filename: "readme.md".into(),
        content_type: "text/markdown".into(),
        size: 1024,
        path: "/tmp/readme.md".into(),
        metadata: Default::default(),
        created_at: chrono::Utc::now(),
    };
    assert_eq!(info.size, 1024);
    let back: FileInfo = serde_json::from_value(serde_json::to_value(&info).unwrap()).unwrap();
    assert_eq!(back.filename, "readme.md");
}

#[test]
fn test_presigned_url() {
    use storage::PresignedUrl;
    let u = PresignedUrl { url: "https://example.com/file".into(), method: "GET".into(), expires_at: chrono::Utc::now() };
    assert_eq!(u.method, "GET");
}
