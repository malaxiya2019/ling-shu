//! LingShu Traits — 综合测试
//!
//! 覆盖所有结构体构建、序列化/反序列化、默认值、Mock trait 实现。

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::*;
use serde_json::Value;
use std::collections::HashMap;

// ════════════════════════════════════════════════════════════════════════
// agent.rs — AgentStatus, AgentSnapshot, AgentOutput
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_agent_status_roundtrip() {
    use agent::AgentStatus;
    for status in &[
        AgentStatus::Idle,
        AgentStatus::Running,
        AgentStatus::Paused,
        AgentStatus::Completed,
        AgentStatus::Failed,
    ] {
        let json = serde_json::to_string(status).unwrap();
        let deserialized: AgentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(*status, deserialized);
    }
}

#[test]
fn test_agent_snapshot_build() {
    use agent::{AgentSnapshot, AgentStatus};
    let id = LsId::new();
    let ctx = LsContext::with_session(id);
    let snap = AgentSnapshot {
        agent_id: id,
        status: AgentStatus::Running,
        context: ctx.clone(),
        state: vec![1, 2, 3],
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&snap).unwrap();
    let deserialized: AgentSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.agent_id, id);
    assert_eq!(deserialized.status, AgentStatus::Running);
    assert_eq!(deserialized.state, vec![1u8, 2, 3]);
}

#[test]
fn test_agent_output_success() {
    use agent::{AgentOutput, AgentStatus};
    let id = LsId::new();
    let output = AgentOutput {
        agent_id: id,
        status: AgentStatus::Completed,
        data: Some(serde_json::json!({"result": "ok"})),
        error: None,
    };
    let json = serde_json::to_string(&output).unwrap();
    let deserialized: AgentOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.agent_id, id);
    assert!(deserialized.data.is_some());
    assert!(deserialized.error.is_none());
}

#[test]
fn test_agent_output_failed() {
    use agent::{AgentOutput, AgentStatus};
    let output = AgentOutput {
        agent_id: LsId::new(),
        status: AgentStatus::Failed,
        data: None,
        error: Some(LsError::Internal("oops".into())),
    };
    let json = serde_json::to_string(&output).unwrap();
    let deserialized: AgentOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.status, AgentStatus::Failed);
    assert_eq!(
        deserialized.error.unwrap().to_string(),
        "internal error: oops"
    );
}

// ════════════════════════════════════════════════════════════════════════
// llm.rs — LlmRole, LlmMessage, LlmRequest, LlmResponse, etc.
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_llm_role_roundtrip() {
    use llm::LlmRole;
    for role in &[
        LlmRole::System,
        LlmRole::User,
        LlmRole::Assistant,
        LlmRole::Tool,
    ] {
        let json = serde_json::to_string(role).unwrap();
        let deserialized: LlmRole = serde_json::from_str(&json).unwrap();
        assert_eq!(*role, deserialized);
    }
}

#[test]
fn test_llm_message_build() {
    use llm::LlmMessage;
    let msg = LlmMessage {
        role: llm::LlmRole::User,
        content: "hello".into(),
        content_parts: None,
        name: None,
        tool_calls: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: LlmMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.role, llm::LlmRole::User);
    assert_eq!(deserialized.content, "hello");
}

#[test]
fn test_content_part_text() {
    use llm::ContentPart;
    let part = ContentPart::text("hello world");
    let json = serde_json::to_string(&part).unwrap();
    // Verify it serializes as untagged: {"type":"text","text":"hello world"}
    let deserialized: ContentPart = serde_json::from_str(&json).unwrap();
    match deserialized {
        ContentPart::Text { text, .. } => assert_eq!(text, "hello world"),
        _ => panic!("expected Text variant"),
    }
}

#[test]
fn test_content_part_image_url() {
    use llm::{ContentPart, ImageUrl};
    let url = ImageUrl::new("https://example.com/img.png").with_detail("auto");
    let part = ContentPart::image_url(url);
    let json = serde_json::to_string(&part).unwrap();
    let deserialized: ContentPart = serde_json::from_str(&json).unwrap();
    match deserialized {
        ContentPart::ImageUrl { image_url, .. } => {
            assert_eq!(image_url.url, "https://example.com/img.png");
            assert_eq!(image_url.detail.as_deref(), Some("auto"));
        }
        _ => panic!("expected ImageUrl variant"),
    }
}

#[test]
fn test_image_url_from_base64() {
    use llm::ImageUrl;
    let url = ImageUrl::from_base64("image/png", "AAAA");
    assert!(url.url.starts_with("data:image/png;base64,"));
}

#[test]
fn test_llm_request_default_fields() {
    use llm::{LlmMessage, LlmRequest, LlmRole};
    let req = LlmRequest {
        model: "gpt-4".into(),
        messages: vec![LlmMessage {
            role: LlmRole::User,
            content: "hi".into(),
            content_parts: None,
            name: None,
            tool_calls: None,
        }],
        temperature: Some(0.7),
        max_tokens: Some(100),
        tools: None,
        stream: false,
    };
    let json = serde_json::to_string(&req).unwrap();
    let deserialized: LlmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.model, "gpt-4");
    assert_eq!(deserialized.temperature, Some(0.7));
    assert!(!deserialized.stream);
}

#[test]
fn test_llm_usage_roundtrip() {
    use llm::LlmUsage;
    let usage = LlmUsage {
        prompt_tokens: 10,
        completion_tokens: 20,
        total_tokens: 30,
    };
    let json = serde_json::to_string(&usage).unwrap();
    let deserialized: LlmUsage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.total_tokens, 30);
}

#[test]
fn test_llm_chunk_roundtrip() {
    use llm::LlmChunk;
    let chunk = LlmChunk {
        content: Some("hello".into()),
        tool_calls: None,
        finish_reason: Some("stop".into()),
    };
    let json = serde_json::to_string(&chunk).unwrap();
    let deserialized: LlmChunk = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.content.as_deref(), Some("hello"));
    assert_eq!(deserialized.finish_reason.as_deref(), Some("stop"));
}

#[test]
fn test_tool_definition_and_call() {
    use llm::{ToolCall, ToolCallFunction, ToolDefinition, ToolFunction, ToolResult};
    let def = ToolDefinition {
        tool_type: "function".into(),
        function: ToolFunction {
            name: "get_weather".into(),
            description: "Get weather".into(),
            parameters: serde_json::json!({"type": "object"}),
        },
    };
    let json = serde_json::to_string(&def).unwrap();
    let deserialized: ToolDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.tool_type, "function");
    assert_eq!(deserialized.function.name, "get_weather");

    let call = ToolCall {
        id: "call_1".into(),
        call_type: "function".into(),
        function: ToolCallFunction {
            name: "get_weather".into(),
            arguments: "{}".into(),
        },
    };
    let json2 = serde_json::to_string(&call).unwrap();
    let deserialized2: ToolCall = serde_json::from_str(&json2).unwrap();
    assert_eq!(deserialized2.id, "call_1");

    let result = ToolResult {
        tool_call_id: "call_1".into(),
        content: "sunny".into(),
    };
    let json3 = serde_json::to_string(&result).unwrap();
    let deserialized3: ToolResult = serde_json::from_str(&json3).unwrap();
    assert_eq!(deserialized3.content, "sunny");
}

// ════════════════════════════════════════════════════════════════════════
// memory.rs — MemoryItem, MemorySearchResult
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_memory_item_roundtrip() {
    use memory::{MemoryItem, MemorySearchResult};
    let item = MemoryItem {
        memory_id: LsId::new(),
        session_id: LsId::new(),
        content: serde_json::json!({"text": "hello"}),
        metadata: [("lang".into(), "zh".into())].into(),
        created_at: chrono::Utc::now(),
        ttl_seconds: Some(3600),
    };
    let json = serde_json::to_string(&item).unwrap();
    let deserialized: MemoryItem = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.memory_id, item.memory_id);
    assert_eq!(deserialized.ttl_seconds, Some(3600));
    assert_eq!(deserialized.metadata.get("lang").unwrap(), "zh");

    // MemorySearchResult
    let result = MemorySearchResult {
        items: vec![item.clone()],
        total: 1,
    };
    let json2 = serde_json::to_string(&result).unwrap();
    let deserialized2: MemorySearchResult = serde_json::from_str(&json2).unwrap();
    assert_eq!(deserialized2.total, 1);
    assert_eq!(deserialized2.items.len(), 1);
}

// ════════════════════════════════════════════════════════════════════════
// tool.rs — PermissionLevel, ToolCategory, SandboxConfig, etc.
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_permission_level_display() {
    use tool::PermissionLevel;
    assert_eq!(PermissionLevel::Public.to_string(), "public");
    assert_eq!(PermissionLevel::Admin.to_string(), "admin");
    assert_eq!(PermissionLevel::SuperAdmin.to_string(), "super_admin");
}

#[test]
fn test_tool_category_display() {
    use tool::ToolCategory;
    assert_eq!(ToolCategory::General.to_string(), "general");
    assert_eq!(ToolCategory::FileSystem.to_string(), "filesystem");
    assert_eq!(
        ToolCategory::Custom("my_tool".into()).to_string(),
        "my_tool"
    );
}

#[test]
fn test_sandbox_config_default() {
    use tool::SandboxConfig;
    let cfg = SandboxConfig::default();
    assert_eq!(cfg.max_execution_ms, 30_000);
    assert_eq!(cfg.max_output_bytes, 1_000_000);
    assert!(!cfg.network_isolated);
    assert!(!cfg.fs_isolated);
    assert!(cfg.max_memory_mb.is_none());
    assert!(cfg.special_permissions.is_empty());
}

#[test]
fn test_tool_info_builder() {
    use tool::{PermissionLevel, SandboxConfig, ToolCategory, ToolInfo};
    let info = ToolInfo::new("calc", "Calculator", vec![])
        .with_category(ToolCategory::General)
        .with_tags(vec!["math".into()])
        .with_permission(PermissionLevel::User)
        .with_timeout(5000)
        .with_sandbox(SandboxConfig::default());
    assert_eq!(info.name, "calc");
    assert_eq!(info.metadata.tags, vec!["math"]);
    assert_eq!(info.metadata.permission_level, PermissionLevel::User);
    assert_eq!(info.metadata.timeout_ms, Some(5000));
    assert!(info.metadata.sandbox_config.is_some());
}

#[test]
fn test_tool_call_record_roundtrip() {
    use tool::ToolCallRecord;
    let rec = ToolCallRecord {
        tool_id: LsId::new(),
        call_id: LsId::new(),
        session_id: LsId::new(),
        input: serde_json::json!({"a": 1}),
        output: serde_json::json!({"result": 2}),
        duration_ms: 42,
        success: true,
        timestamp: chrono::Utc::now(),
        caller: Some("alice".into()),
        error: None,
    };
    let json = serde_json::to_string(&rec).unwrap();
    let deserialized: ToolCallRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.duration_ms, 42);
    assert!(deserialized.success);
    assert_eq!(deserialized.caller.as_deref(), Some("alice"));
}

// ════════════════════════════════════════════════════════════════════════
// scheduler.rs — Priority, TaskStatus, TaskInfo, QuotaInfo
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_priority_roundtrip() {
    use scheduler::Priority;
    for p in &[
        Priority::Low,
        Priority::Normal,
        Priority::High,
        Priority::Critical,
    ] {
        let json = serde_json::to_string(p).unwrap();
        let deserialized: Priority = serde_json::from_str(&json).unwrap();
        assert_eq!(*p, deserialized);
    }
}

#[test]
fn test_task_status_roundtrip() {
    use scheduler::TaskStatus;
    for s in &[
        TaskStatus::Pending,
        TaskStatus::Running,
        TaskStatus::Completed,
        TaskStatus::Failed,
        TaskStatus::Cancelled,
    ] {
        let json = serde_json::to_string(s).unwrap();
        let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(*s, deserialized);
    }
}

#[test]
fn test_task_info_build() {
    use scheduler::{Priority, TaskInfo, TaskStatus};
    let info = TaskInfo {
        task_id: LsId::new(),
        session_id: LsId::new(),
        priority: Priority::High,
        status: TaskStatus::Pending,
        created_at: chrono::Utc::now(),
        tags: [("env".into(), "prod".into())].into(),
    };
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: TaskInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.priority, Priority::High);
    assert_eq!(deserialized.tags.get("env").unwrap(), "prod");
}

#[test]
fn test_quota_info() {
    use scheduler::QuotaInfo;
    let q = QuotaInfo {
        max_concurrent: 10,
        current_concurrent: 3,
        max_queue_size: 100,
        current_queue_size: 5,
    };
    assert_eq!(q.max_concurrent, 10);
    assert_eq!(q.current_queue_size, 5);
}

// ════════════════════════════════════════════════════════════════════════
// event_bus.rs — Event, SubscriptionInfo
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_event_roundtrip() {
    use event_bus::Event;
    let event = Event {
        event_id: "evt_1".into(),
        topic: "agent.started".into(),
        session_id: "sess_1".into(),
        trace_id: "trace_1".into(),
        payload: serde_json::json!({"agent_id": "a1"}),
        timestamp: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.event_id, "evt_1");
    assert_eq!(deserialized.topic, "agent.started");
    assert_eq!(deserialized.payload["agent_id"], "a1");
}

#[test]
fn test_subscription_info() {
    use event_bus::SubscriptionInfo;
    let sub = SubscriptionInfo {
        id: "sub_1".into(),
        topic_pattern: "agent.*".into(),
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&sub).unwrap();
    let deserialized: SubscriptionInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "sub_1");
    assert_eq!(deserialized.topic_pattern, "agent.*");
}

// ════════════════════════════════════════════════════════════════════════
// storage.rs — FileInfo, PresignedUrl
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_file_info_roundtrip() {
    use storage::FileInfo;
    let fi = FileInfo {
        file_id: LsId::new(),
        filename: "test.txt".into(),
        content_type: "text/plain".into(),
        size: 100,
        path: "/tmp/test.txt".into(),
        metadata: [("author".into(), "me".into())].into(),
        created_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&fi).unwrap();
    let deserialized: FileInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.filename, "test.txt");
    assert_eq!(deserialized.size, 100);
    assert_eq!(deserialized.metadata.get("author").unwrap(), "me");
}

#[test]
fn test_presigned_url() {
    use storage::PresignedUrl;
    let url = PresignedUrl {
        url: "https://example.com/upload".into(),
        method: "PUT".into(),
        expires_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&url).unwrap();
    let deserialized: PresignedUrl = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.url, "https://example.com/upload");
    assert_eq!(deserialized.method, "PUT");
}

// ════════════════════════════════════════════════════════════════════════
// vector_store.rs — VectorCollection, VectorRecord, VectorSearchResult
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_vector_collection_roundtrip() {
    use vector_store::VectorCollection;
    let vc = VectorCollection {
        collection_id: LsId::new(),
        name: "docs".into(),
        dimensions: 768,
        metadata: serde_json::json!({"engine": "hnsw"}),
    };
    let json = serde_json::to_string(&vc).unwrap();
    let deserialized: VectorCollection = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "docs");
    assert_eq!(deserialized.dimensions, 768);
}

#[test]
fn test_vector_record() {
    use vector_store::VectorRecord;
    let vr = VectorRecord {
        id: LsId::new(),
        vector: vec![0.1, 0.2, 0.3],
        metadata: serde_json::json!({"source": "doc1"}),
        score: Some(0.95),
    };
    let json = serde_json::to_string(&vr).unwrap();
    let deserialized: VectorRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.vector.len(), 3);
    assert_eq!(deserialized.score, Some(0.95));
}

#[test]
fn test_vector_search_result() {
    use vector_store::VectorSearchResult;
    let vsr = VectorSearchResult {
        records: vec![],
        total: 0,
    };
    let json = serde_json::to_string(&vsr).unwrap();
    let deserialized: VectorSearchResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.total, 0);
    assert!(deserialized.records.is_empty());
}

// ════════════════════════════════════════════════════════════════════════
// database.rs — Pagination, PaginatedResult, QueryFilter
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_pagination() {
    use database::Pagination;
    let p = Pagination {
        page: 1,
        page_size: 20,
    };
    assert_eq!(p.page, 1);
}

#[test]
fn test_paginated_result() {
    use database::PaginatedResult;
    let pr = PaginatedResult {
        items: vec![serde_json::json!({"id": 1})],
        total: 1,
        page: 1,
        page_size: 20,
        total_pages: 1,
    };
    let json = serde_json::to_string(&pr).unwrap();
    let deserialized: PaginatedResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.total, 1);
    assert_eq!(deserialized.items.len(), 1);
}

#[test]
fn test_query_filter() {
    use database::QueryFilter;
    let qf = QueryFilter {
        field: "name".into(),
        operator: "eq".into(),
        value: serde_json::json!("alice"),
    };
    let json = serde_json::to_string(&qf).unwrap();
    let deserialized: QueryFilter = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.field, "name");
    assert_eq!(deserialized.operator, "eq");
}

// ════════════════════════════════════════════════════════════════════════
// embedding.rs — EmbeddingVector, EmbeddingRequest, EmbeddingResponse
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_embedding_vector() {
    use embedding::EmbeddingVector;
    let ev = EmbeddingVector {
        dimensions: 3,
        values: vec![0.1, 0.2, 0.3],
    };
    let json = serde_json::to_string(&ev).unwrap();
    let deserialized: EmbeddingVector = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.dimensions, 3);
    assert_eq!(deserialized.values.len(), 3);
}

#[test]
fn test_embedding_request() {
    use embedding::EmbeddingRequest;
    let req = EmbeddingRequest {
        input: vec!["hello".into(), "world".into()],
        model: Some("text-embedding-3".into()),
    };
    let json = serde_json::to_string(&req).unwrap();
    let deserialized: EmbeddingRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.input.len(), 2);
}

#[test]
fn test_embedding_response() {
    use embedding::{EmbeddingResponse, EmbeddingUsage, EmbeddingVector};
    let resp = EmbeddingResponse {
        vectors: vec![EmbeddingVector {
            dimensions: 3,
            values: vec![0.1, 0.2, 0.3],
        }],
        model: "text-embedding-3".into(),
        usage: EmbeddingUsage { total_tokens: 10 },
    };
    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: EmbeddingResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.vectors.len(), 1);
    assert_eq!(deserialized.usage.total_tokens, 10);
}

// ════════════════════════════════════════════════════════════════════════
// plugin.rs — PluginStatus, PluginPermission, PluginManifest, PluginInfo
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_plugin_status_roundtrip() {
    use plugin::PluginStatus;
    for (input, expected_str) in &[
        (PluginStatus::Installed, "Installed"),
        (PluginStatus::Loaded, "Loaded"),
        (PluginStatus::Running, "Running"),
        (PluginStatus::Stopped, "Stopped"),
    ] {
        let json = serde_json::to_string(input).unwrap();
        assert_eq!(json.trim_matches('"'), *expected_str);
        let deserialized: PluginStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(*input, deserialized);
    }
    // Failed variant contains a string
    let failed = PluginStatus::Failed("error".into());
    let json = serde_json::to_string(&failed).unwrap();
    let deserialized: PluginStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, PluginStatus::Failed("error".into()));
}

#[test]
fn test_plugin_permission() {
    use plugin::PluginPermission;
    let perm = PluginPermission {
        resource: "storage".into(),
        actions: vec!["read".into(), "write".into()],
    };
    let json = serde_json::to_string(&perm).unwrap();
    let deserialized: PluginPermission = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.resource, "storage");
    assert_eq!(deserialized.actions.len(), 2);
}

#[test]
fn test_plugin_manifest_default() {
    use plugin::PluginManifest;
    let m = PluginManifest::default();
    assert_eq!(m.version, "1.0.0");
    assert_eq!(m.plugin_type, "static");
    assert!(m.capabilities.is_empty());
    assert!(m.tools.is_empty());
}

#[test]
fn test_plugin_manifest_full() {
    use plugin::{Capability, PluginManifest, PluginPermission, ToolDeclaration};
    let m = PluginManifest {
        name: "my-plugin".into(),
        version: "2.0.0".into(),
        description: "Test plugin".into(),
        author: Some("alice".into()),
        homepage: Some("https://example.com".into()),
        license: Some("MIT".into()),
        plugin_type: "dynamic".into(),
        entry_point: Some("my_plugin_init".into()),
        permissions: vec![PluginPermission {
            resource: "network".into(),
            actions: vec!["connect".into()],
        }],
        min_api_version: Some("1.0.0".into()),
        capabilities: vec![Capability {
            name: "search".into(),
            description: Some("search engine".into()),
            version_req: None,
        }],
        tools: vec![ToolDeclaration {
            name: "search_tool".into(),
            description: "Searches".into(),
            permission_level: "user".into(),
        }],
    };
    let json = serde_json::to_string(&m).unwrap();
    let deserialized: PluginManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "my-plugin");
    assert_eq!(deserialized.version, "2.0.0");
    assert_eq!(deserialized.capabilities.len(), 1);
    assert_eq!(deserialized.tools.len(), 1);
}

#[test]
fn test_plugin_info_default() {
    use plugin::PluginInfo;
    let info = PluginInfo::default();
    assert!(!info.plugin_id.is_nil());
    assert!(info.loaded_at.is_none());
}

// ════════════════════════════════════════════════════════════════════════
// knowledge.rs — KnowledgeEntry, DataSource, KnowledgeSearchResult
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_knowledge_entry() {
    use knowledge::KnowledgeEntry;
    let entry = KnowledgeEntry {
        entry_id: LsId::new(),
        source: "web".into(),
        content: serde_json::json!({"title": "hello"}),
        version: 1,
        metadata: [("lang".into(), "en".into())].into(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: KnowledgeEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.source, "web");
    assert_eq!(deserialized.version, 1);
}

#[test]
fn test_data_source() {
    use knowledge::DataSource;
    let ds = DataSource {
        source_id: LsId::new(),
        name: "docs".into(),
        source_type: "file".into(),
        config: serde_json::json!({"path": "/tmp"}),
    };
    let json = serde_json::to_string(&ds).unwrap();
    let deserialized: DataSource = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "docs");
}

#[test]
fn test_knowledge_search_result() {
    use knowledge::KnowledgeSearchResult;
    let result = KnowledgeSearchResult {
        entries: vec![],
        total: 0,
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: KnowledgeSearchResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.total, 0);
}

// ════════════════════════════════════════════════════════════════════════
// voice.rs — TtsRequest, TtsResponse, SttRequest, SttResponse, SttSegment
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_tts_request_default() {
    use voice::TtsRequest;
    let req = TtsRequest::default();
    assert_eq!(req.speed, 1.0);
    assert_eq!(req.format, "wav");
    assert!(req.text.is_empty());
}

#[test]
fn test_tts_request_custom() {
    use voice::TtsRequest;
    let req = TtsRequest {
        text: "hello".into(),
        model: Some("tts-1".into()),
        voice: Some("alloy".into()),
        speed: 1.5,
        format: "mp3".into(),
        language: Some("en-US".into()),
    };
    let json = serde_json::to_string(&req).unwrap();
    let deserialized: TtsRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.text, "hello");
    assert_eq!(deserialized.speed, 1.5);
    assert_eq!(deserialized.format, "mp3");
}

#[test]
fn test_tts_response() {
    use voice::TtsResponse;
    let resp = TtsResponse {
        audio_data: vec![0, 1, 2],
        format: "wav".into(),
        duration_secs: 2.5,
        sample_rate: 44100,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: TtsResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.duration_secs, 2.5);
    assert_eq!(deserialized.sample_rate, 44100);
}

#[test]
fn test_stt_request() {
    use voice::SttRequest;
    let req = SttRequest {
        audio_data: vec![0u8; 100],
        format: "wav".into(),
        model: Some("whisper-1".into()),
        language: Some("zh-CN".into()),
        punctuate: true,
    };
    let json = serde_json::to_string(&req).unwrap();
    let deserialized: SttRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.format, "wav");
    assert!(deserialized.punctuate);
}

#[test]
fn test_stt_response() {
    use voice::{SttResponse, SttSegment};
    let resp = SttResponse {
        text: "你好世界".into(),
        language: Some("zh-CN".into()),
        confidence: 0.98,
        segments: vec![SttSegment {
            start: 0.0,
            end: 1.0,
            text: "你好".into(),
            confidence: 0.99,
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: SttResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.text, "你好世界");
    assert_eq!(deserialized.segments.len(), 1);
}

// ════════════════════════════════════════════════════════════════════════
// repository.rs — DatabaseRepository 骨架 + Repository trait
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_database_repository_collection_name() {
    use repository::DatabaseRepository;
    let repo = DatabaseRepository::<serde_json::Value>::new("users");
    assert_eq!(repo.collection_name(), "users");
}

// ════════════════════════════════════════════════════════════════════════
// runtime.rs — RuntimeStatus, RuntimeStats
// ════════════════════════════════════════════════════════════════════════

#[test]
fn test_runtime_status_roundtrip() {
    use runtime::RuntimeStatus;
    for s in &[
        RuntimeStatus::Uninitialized,
        RuntimeStatus::Initializing,
        RuntimeStatus::Running,
        RuntimeStatus::Paused,
        RuntimeStatus::ShuttingDown,
        RuntimeStatus::Stopped,
    ] {
        let json = serde_json::to_string(s).unwrap();
        let deserialized: RuntimeStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(*s, deserialized);
    }
}

#[test]
fn test_runtime_stats() {
    use runtime::RuntimeStats;
    let stats = RuntimeStats {
        uptime_seconds: 3600,
        active_sessions: 5,
        active_tasks: 2,
        total_tasks_completed: 100,
        total_tasks_failed: 3,
    };
    let json = serde_json::to_string(&stats).unwrap();
    let deserialized: RuntimeStats = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.uptime_seconds, 3600);
    assert_eq!(deserialized.total_tasks_completed, 100);
}

// ════════════════════════════════════════════════════════════════════════
// Mock trait 实现 — 验证 trait bounds 满足 Send + Sync + 'static
// ════════════════════════════════════════════════════════════════════════

// ── Mock Agent ──
struct MockAgent {
    id: LsId,
    status: agent::AgentStatus,
}

#[async_trait::async_trait]
impl agent::Agent for MockAgent {
    fn id(&self) -> LsId {
        self.id
    }
    async fn run(&mut self, _ctx: LsContext, _input: Value) -> LsResult<agent::AgentOutput> {
        self.status = agent::AgentStatus::Running;
        Ok(agent::AgentOutput {
            agent_id: self.id,
            status: self.status,
            data: None,
            error: None,
        })
    }
    async fn pause(&mut self, _ctx: LsContext) -> LsResult<()> {
        self.status = agent::AgentStatus::Paused;
        Ok(())
    }
    async fn resume(&mut self, _ctx: LsContext) -> LsResult<()> {
        self.status = agent::AgentStatus::Running;
        Ok(())
    }
    async fn cancel(&mut self, _ctx: LsContext) -> LsResult<()> {
        self.status = agent::AgentStatus::Failed;
        Ok(())
    }
    async fn snapshot(&self, _ctx: LsContext) -> LsResult<agent::AgentSnapshot> {
        Ok(agent::AgentSnapshot {
            agent_id: self.id,
            status: self.status,
            context: LsContext::with_session(self.id),
            state: vec![],
            created_at: chrono::Utc::now(),
        })
    }
    async fn restore(&mut self, _ctx: LsContext, snapshot: agent::AgentSnapshot) -> LsResult<()> {
        self.status = snapshot.status;
        Ok(())
    }
    async fn status(&self, _ctx: LsContext) -> LsResult<agent::AgentStatus> {
        Ok(self.status)
    }
}

#[test]
fn test_mock_agent_lifecycle() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut agent = MockAgent {
            id: LsId::new(),
            status: agent::AgentStatus::Idle,
        };
        let ctx = LsContext::with_session(LsId::new());

        assert_eq!(
            agent.status(ctx.clone()).await.unwrap(),
            agent::AgentStatus::Idle
        );

        let output = agent
            .run(ctx.clone(), serde_json::json!("test"))
            .await
            .unwrap();
        assert_eq!(output.status, agent::AgentStatus::Running);

        agent.pause(ctx.clone()).await.unwrap();
        assert_eq!(
            agent.status(ctx.clone()).await.unwrap(),
            agent::AgentStatus::Paused
        );

        agent.resume(ctx.clone()).await.unwrap();
        assert_eq!(
            agent.status(ctx.clone()).await.unwrap(),
            agent::AgentStatus::Running
        );

        // snapshot & restore
        let snap = agent.snapshot(ctx.clone()).await.unwrap();
        assert_eq!(snap.agent_id, agent.id);

        agent.cancel(ctx.clone()).await.unwrap();
        agent.restore(ctx.clone(), snap).await.unwrap();
        assert_eq!(
            agent.status(ctx.clone()).await.unwrap(),
            agent::AgentStatus::Running
        );
    });
}

#[test]
fn test_mock_agent_default_methods() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut agent = MockAgent {
            id: LsId::new(),
            status: agent::AgentStatus::Idle,
        };
        let ctx = LsContext::with_session(LsId::new());

        let result = agent.restart(ctx.clone()).await;
        assert!(result.is_err()); // default returns Unsupported

        let result = agent
            .update_config(ctx.clone(), serde_json::json!({}))
            .await;
        assert!(result.is_err()); // default returns Unsupported
    });
}

#[test]
fn test_box_agent_blanket_impl() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let agent: Box<dyn agent::Agent> = Box::new(MockAgent {
            id: LsId::new(),
            status: agent::AgentStatus::Idle,
        });
        let ctx = LsContext::with_session(LsId::new());
        let status = agent.status(ctx).await.unwrap();
        assert_eq!(status, agent::AgentStatus::Idle);
    });
}

// ── Mock Memory ──
struct MockMemory {
    items: std::sync::Mutex<Vec<memory::MemoryItem>>,
}

#[async_trait::async_trait]
impl memory::Memory for MockMemory {
    async fn write(&self, _ctx: LsContext, item: memory::MemoryItem) -> LsResult<LsId> {
        let id = item.memory_id;
        self.items.lock().unwrap().push(item);
        Ok(id)
    }
    async fn write_batch(
        &self,
        _ctx: LsContext,
        items: Vec<memory::MemoryItem>,
    ) -> LsResult<Vec<LsId>> {
        let ids: Vec<LsId> = items.iter().map(|i| i.memory_id).collect();
        self.items.lock().unwrap().extend(items);
        Ok(ids)
    }
    async fn read(&self, _ctx: LsContext, memory_id: LsId) -> LsResult<memory::MemoryItem> {
        let items = self.items.lock().unwrap();
        items
            .iter()
            .find(|i| i.memory_id == memory_id)
            .cloned()
            .ok_or_else(|| LsError::NotFound("memory not found".into()))
    }
    async fn search(
        &self,
        _ctx: LsContext,
        query: &str,
        limit: u64,
    ) -> LsResult<memory::MemorySearchResult> {
        let items = self.items.lock().unwrap();
        let filtered: Vec<_> = items
            .iter()
            .filter(|i| i.content.to_string().contains(query))
            .take(limit as usize)
            .cloned()
            .collect();
        let total = filtered.len() as u64;
        Ok(memory::MemorySearchResult {
            items: filtered,
            total,
        })
    }
    async fn delete(&self, _ctx: LsContext, memory_id: LsId) -> LsResult<()> {
        let mut items = self.items.lock().unwrap();
        items.retain(|i| i.memory_id != memory_id);
        Ok(())
    }
    async fn clean_expired(&self, _ctx: LsContext) -> LsResult<u64> {
        let mut items = self.items.lock().unwrap();
        let before = items.len();
        let now = chrono::Utc::now();
        items.retain(|i| {
            if let Some(ttl) = i.ttl_seconds {
                let elapsed = (now - i.created_at).num_seconds() as u64;
                elapsed < ttl
            } else {
                true
            }
        });
        Ok((before - items.len()) as u64)
    }
    async fn clear_session(&self, _ctx: LsContext, session_id: LsId) -> LsResult<()> {
        let mut items = self.items.lock().unwrap();
        items.retain(|i| i.session_id != session_id);
        Ok(())
    }
}

#[test]
fn test_mock_memory_crud() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mem = MockMemory {
            items: std::sync::Mutex::new(vec![]),
        };
        let ctx = LsContext::with_session(LsId::new());

        let item = memory::MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx.session_id,
            content: serde_json::json!("hello world"),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            ttl_seconds: None,
        };
        let id = mem.write(ctx.clone(), item.clone()).await.unwrap();
        assert_eq!(id, item.memory_id);

        let read_back = mem.read(ctx.clone(), id).await.unwrap();
        assert_eq!(read_back.content, item.content);

        let search = mem.search(ctx.clone(), "hello", 10).await.unwrap();
        assert_eq!(search.total, 1);
    });
}

#[test]
fn test_mock_memory_clean_expired() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mem = MockMemory {
            items: std::sync::Mutex::new(vec![]),
        };
        let ctx = LsContext::with_session(LsId::new());

        // Insert an expired item (ttl=0, should be cleaned immediately)
        let expired = memory::MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx.session_id,
            content: serde_json::json!("expired"),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            ttl_seconds: Some(0),
        };
        mem.write(ctx.clone(), expired).await.unwrap();
        let cleaned = mem.clean_expired(ctx.clone()).await.unwrap();
        assert_eq!(cleaned, 1);

        let search = mem.search(ctx.clone(), "expired", 10).await.unwrap();
        assert_eq!(search.total, 0);
    });
}

// ── Mock Llm ──
struct MockLlm;

#[async_trait::async_trait]
impl llm::Llm for MockLlm {
    async fn invoke(
        &self,
        _ctx: LsContext,
        request: llm::LlmRequest,
    ) -> LsResult<llm::LlmResponse> {
        Ok(llm::LlmResponse {
            message: llm::LlmMessage {
                role: llm::LlmRole::Assistant,
                content: format!(
                    "echo: {}",
                    request.messages.first().map_or("", |m| &m.content)
                ),
                content_parts: None,
                name: None,
                tool_calls: None,
            },
            finish_reason: "stop".into(),
            usage: llm::LlmUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        })
    }
    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        _request: llm::LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<llm::LlmChunk>>> {
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tx.send(Ok(llm::LlmChunk {
            content: Some("hello".into()),
            tool_calls: None,
            finish_reason: None,
        }))
        .await
        .unwrap();
        tx.send(Ok(llm::LlmChunk {
            content: None,
            tool_calls: None,
            finish_reason: Some("stop".into()),
        }))
        .await
        .unwrap();
        Ok(rx)
    }
    async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let mut m = HashMap::new();
        m.insert("total_tokens".into(), 100);
        Ok(m)
    }
}

#[test]
fn test_mock_llm_invoke() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let llm = MockLlm;
        let ctx = LsContext::with_session(LsId::new());
        let req = llm::LlmRequest {
            model: "gpt-4".into(),
            messages: vec![llm::LlmMessage {
                role: llm::LlmRole::User,
                content: "hi".into(),
                content_parts: None,
                name: None,
                tool_calls: None,
            }],
            temperature: None,
            max_tokens: None,
            tools: None,
            stream: false,
        };
        let resp = llm.invoke(ctx.clone(), req).await.unwrap();
        assert_eq!(resp.message.content, "echo: hi");
        assert_eq!(resp.usage.total_tokens, 15);
    });
}

#[test]
fn test_mock_llm_stream() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let llm = MockLlm;
        let ctx = LsContext::with_session(LsId::new());
        let req = llm::LlmRequest {
            model: "gpt-4".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            tools: None,
            stream: true,
        };
        let mut rx = llm.invoke_stream(ctx.clone(), req).await.unwrap();
        let first = rx.recv().await.unwrap().unwrap();
        assert_eq!(first.content.as_deref(), Some("hello"));
    });
}

// ── Mock EventBus ──
struct MockEventBus {
    events: std::sync::Mutex<Vec<event_bus::Event>>,
}

#[async_trait::async_trait]
impl event_bus::EventBus for MockEventBus {
    async fn publish(&self, _ctx: LsContext, event: event_bus::Event) -> LsResult<()> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
    async fn publish_batch(&self, _ctx: LsContext, events: Vec<event_bus::Event>) -> LsResult<()> {
        self.events.lock().unwrap().extend(events);
        Ok(())
    }
    async fn subscribe(
        &self,
        _ctx: LsContext,
        topic_pattern: &str,
        _handler: event_bus::EventHandler,
    ) -> LsResult<String> {
        Ok(format!("sub_{}", topic_pattern))
    }
    async fn unsubscribe(&self, _ctx: LsContext, _subscription_id: &str) -> LsResult<()> {
        Ok(())
    }
    async fn list_subscriptions(
        &self,
        _ctx: LsContext,
    ) -> LsResult<Vec<event_bus::SubscriptionInfo>> {
        Ok(vec![])
    }
}

#[test]
fn test_mock_eventbus() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let bus = MockEventBus {
            events: std::sync::Mutex::new(vec![]),
        };
        let ctx = LsContext::with_session(LsId::new());
        let event = event_bus::Event {
            event_id: "e1".into(),
            topic: "test".into(),
            session_id: "s1".into(),
            trace_id: "t1".into(),
            payload: serde_json::json!({"key": "val"}),
            timestamp: chrono::Utc::now(),
        };
        bus.publish(ctx.clone(), event.clone()).await.unwrap();

        let sub_id = bus
            .subscribe(ctx.clone(), "test.*", Box::new(|_e| Ok(())))
            .await
            .unwrap();
        assert_eq!(sub_id, "sub_test.*");

        let subscriptions = bus.list_subscriptions(ctx.clone()).await.unwrap();
        assert_eq!(subscriptions.len(), 0);
    });
}

// ── Mock Scheduler ──
struct MockScheduler;

#[async_trait::async_trait]
impl scheduler::Scheduler for MockScheduler {
    async fn submit(&self, _ctx: LsContext, _task: Box<dyn FnOnce() + Send>) -> LsResult<LsId> {
        Ok(LsId::new())
    }
    async fn cancel(&self, _ctx: LsContext, _task_id: LsId) -> LsResult<()> {
        Ok(())
    }
    async fn get_task(&self, _ctx: LsContext, task_id: LsId) -> LsResult<scheduler::TaskInfo> {
        Ok(scheduler::TaskInfo {
            task_id,
            session_id: LsId::new(),
            priority: scheduler::Priority::Normal,
            status: scheduler::TaskStatus::Completed,
            created_at: chrono::Utc::now(),
            tags: HashMap::new(),
        })
    }
    async fn pause(&self, _ctx: LsContext) -> LsResult<()> {
        Ok(())
    }
    async fn resume(&self, _ctx: LsContext) -> LsResult<()> {
        Ok(())
    }
    async fn quota(&self, _ctx: LsContext) -> LsResult<scheduler::QuotaInfo> {
        Ok(scheduler::QuotaInfo {
            max_concurrent: 10,
            current_concurrent: 0,
            max_queue_size: 100,
            current_queue_size: 0,
        })
    }
}

#[test]
fn test_mock_scheduler() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let sched = MockScheduler;
        let ctx = LsContext::with_session(LsId::new());
        let id = sched.submit(ctx.clone(), Box::new(|| {})).await.unwrap();
        assert!(!id.is_nil());

        let task = sched.get_task(ctx.clone(), id).await.unwrap();
        assert_eq!(task.status, scheduler::TaskStatus::Completed);

        let q = sched.quota(ctx.clone()).await.unwrap();
        assert_eq!(q.max_concurrent, 10);
    });
}

// ── Mock VectorStore ──
struct MockVectorStore;

#[async_trait::async_trait]
impl vector_store::VectorStore for MockVectorStore {
    async fn create_collection(
        &self,
        _ctx: LsContext,
        _name: &str,
        _dimensions: usize,
    ) -> LsResult<LsId> {
        Ok(LsId::new())
    }
    async fn delete_collection(&self, _ctx: LsContext, _collection_id: LsId) -> LsResult<()> {
        Ok(())
    }
    async fn upsert(
        &self,
        _ctx: LsContext,
        _collection_id: LsId,
        _records: Vec<vector_store::VectorRecord>,
    ) -> LsResult<()> {
        Ok(())
    }
    async fn search(
        &self,
        _ctx: LsContext,
        _collection_id: LsId,
        _query: Vec<f32>,
        top_k: u64,
    ) -> LsResult<vector_store::VectorSearchResult> {
        Ok(vector_store::VectorSearchResult {
            records: vec![],
            total: top_k,
        })
    }
}

#[test]
fn test_mock_vector_store() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let vs = MockVectorStore;
        let ctx = LsContext::with_session(LsId::new());
        let id = vs
            .create_collection(ctx.clone(), "my_collection", 768)
            .await
            .unwrap();
        assert!(!id.is_nil());

        let result = vs.search(ctx.clone(), id, vec![0.1, 0.2], 5).await.unwrap();
        assert_eq!(result.total, 5);
    });
}

// ── Mock Embedding ──
struct MockEmbedding;

#[async_trait::async_trait]
impl embedding::Embedding for MockEmbedding {
    async fn embed(
        &self,
        _ctx: LsContext,
        request: embedding::EmbeddingRequest,
    ) -> LsResult<embedding::EmbeddingResponse> {
        Ok(embedding::EmbeddingResponse {
            vectors: request
                .input
                .iter()
                .map(|_| embedding::EmbeddingVector {
                    dimensions: self.dimensions(),
                    values: vec![0.1; self.dimensions()],
                })
                .collect(),
            model: "mock".into(),
            usage: embedding::EmbeddingUsage {
                total_tokens: request.input.len() as u64,
            },
        })
    }
    fn validate_dimensions(&self, vector: &embedding::EmbeddingVector) -> LsResult<()> {
        if vector.dimensions == self.dimensions() {
            Ok(())
        } else {
            Err(LsError::InvalidArgument("dimension mismatch".into()))
        }
    }
    fn dimensions(&self) -> usize {
        3
    }
}

#[test]
fn test_mock_embedding() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let emb = MockEmbedding;
        let ctx = LsContext::with_session(LsId::new());
        let resp = emb
            .embed(
                ctx.clone(),
                embedding::EmbeddingRequest {
                    input: vec!["hello".into(), "world".into()],
                    model: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(resp.vectors.len(), 2);
        assert_eq!(resp.vectors[0].dimensions, 3);

        let vec = embedding::EmbeddingVector {
            dimensions: 3,
            values: vec![0.1, 0.2, 0.3],
        };
        assert!(emb.validate_dimensions(&vec).is_ok());

        let wrong = embedding::EmbeddingVector {
            dimensions: 10,
            values: vec![0.0; 10],
        };
        assert!(emb.validate_dimensions(&wrong).is_err());
    });
}

// ── Mock Database ──
struct MockDatabase {
    store: std::sync::Mutex<HashMap<String, Value>>,
}

#[async_trait::async_trait]
impl database::Database for MockDatabase {
    async fn insert(&self, _ctx: LsContext, _collection: &str, data: Value) -> LsResult<Value> {
        self.store
            .lock()
            .unwrap()
            .insert("key".into(), data.clone());
        Ok(data)
    }
    async fn get_by_id(
        &self,
        _ctx: LsContext,
        _collection: &str,
        id: &str,
    ) -> LsResult<Option<Value>> {
        Ok(self.store.lock().unwrap().get(id).cloned())
    }
    async fn query(
        &self,
        _ctx: LsContext,
        _collection: &str,
        _filters: Vec<database::QueryFilter>,
        pagination: database::Pagination,
    ) -> LsResult<database::PaginatedResult> {
        let items: Vec<Value> = self.store.lock().unwrap().values().cloned().collect();
        let total = items.len() as u64;
        Ok(database::PaginatedResult {
            items,
            total,
            page: pagination.page,
            page_size: pagination.page_size,
            total_pages: (total + pagination.page_size - 1) / pagination.page_size,
        })
    }
    async fn update(
        &self,
        _ctx: LsContext,
        _collection: &str,
        id: &str,
        data: Value,
    ) -> LsResult<Option<Value>> {
        Ok(self.store.lock().unwrap().insert(id.into(), data))
    }
    async fn delete(&self, _ctx: LsContext, _collection: &str, id: &str) -> LsResult<bool> {
        Ok(self.store.lock().unwrap().remove(id).is_some())
    }
    async fn begin_transaction(&self, _ctx: LsContext) -> LsResult<String> {
        Ok("txn_1".into())
    }
    async fn commit_transaction(&self, _ctx: LsContext, _txn_id: &str) -> LsResult<()> {
        Ok(())
    }
    async fn rollback_transaction(&self, _ctx: LsContext, _txn_id: &str) -> LsResult<()> {
        Ok(())
    }
}

#[test]
fn test_mock_database_crud() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let db = MockDatabase {
            store: std::sync::Mutex::new(HashMap::new()),
        };
        let ctx = LsContext::with_session(LsId::new());
        let data = serde_json::json!({"name": "alice"});
        db.insert(ctx.clone(), "users", data.clone()).await.unwrap();

        let query = db
            .query(
                ctx.clone(),
                "users",
                vec![],
                database::Pagination {
                    page: 1,
                    page_size: 10,
                },
            )
            .await
            .unwrap();
        assert_eq!(query.total, 1);
    });
}

// ════════════════════════════════════════════════════════════════════════
// Mock Tool — Tool trait
// ════════════════════════════════════════════════════════════════════════

struct MockTool;

#[async_trait::async_trait]
impl tool::Tool for MockTool {
    fn info(&self) -> tool::ToolInfo {
        tool::ToolInfo::new(
            "mock_tool",
            "A mock tool",
            vec![tool::ToolParam {
                name: "input".into(),
                description: "Input value".into(),
                required: true,
                param_type: "string".into(),
            }],
        )
    }
    fn validate(&self, input: &Value) -> LsResult<()> {
        if input.get("input").is_some() {
            Ok(())
        } else {
            Err(LsError::InvalidArgument("missing input".into()))
        }
    }
    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        Ok(serde_json::json!({"result": input}))
    }
    fn duplicate(&self) -> Box<dyn tool::Tool> {
        Box::new(MockTool)
    }
}

#[test]
fn test_mock_tool() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let tool = MockTool;
        let info = tool.info();
        assert_eq!(info.name, "mock_tool");

        assert!(tool.validate(&serde_json::json!({"input": "test"})).is_ok());
        assert!(tool.validate(&serde_json::json!({})).is_err());

        let ctx = LsContext::with_session(LsId::new());
        let result = tool
            .execute(ctx.clone(), serde_json::json!({"input": "hello"}))
            .await
            .unwrap();
        assert_eq!(result["result"]["input"], "hello");

        let dup = tool.duplicate();
        assert_eq!(dup.info().name, "mock_tool");
    });
}
