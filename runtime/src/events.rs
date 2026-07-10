//! Runtime Events — 通过 EventBus 发布运行时事件.
//!
//! 提供统一的辅助函数，在 Agent/Tool/Session/Workflow 操作时发布结构化事件。
//! 这些事件通过 EventBus → EventBridge → WebSocket / SSE 推送到前端。

use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_eventbus::{EventTopic, LsEvent};
use lingshu_traits::event_bus::EventBus;
use serde_json::{json, Value};


/// 发布 Agent 状态变更事件.
pub async fn emit_agent_state_change(
    bus: &dyn EventBus,
    ctx: &LsContext,
    agent_id: &LsId,
    agent_name: &str,
    old_state: &str,
    new_state: &str,
) {
    let payload = json!({
        "agent_id": agent_id.to_string(),
        "agent_name": agent_name,
        "old_state": old_state,
        "new_state": new_state,
    });
    let event = LsEvent::new(
        EventTopic::new("agent", "state", "changed"),
        ctx.session_id.to_string(),
        ctx.trace_id.to_string(),
        payload,
    );
    let _ = bus.publish(ctx.clone(), from_ls_event(event)).await;
}

/// 发布工具调用事件.
pub async fn emit_tool_call(
    bus: &dyn EventBus,
    ctx: &LsContext,
    tool_name: &str,
    input: &Value,
    result: &LsResult<Value>,
) {
    let status = match result {
        Ok(_) => "success",
        Err(_) => "failed",
    };
    let output_preview = match result {
        Ok(v) => json!({ "preview": serde_json::to_string(v).unwrap_or_default().chars().take(200).collect::<String>() }),
        Err(e) => json!({ "error": e.to_string() }),
    };
    let payload = json!({
        "tool_name": tool_name,
        "input": input,
        "status": status,
        "output": output_preview,
    });
    let event = LsEvent::new(
        EventTopic::new("tool", "call", "executed"),
        ctx.session_id.to_string(),
        ctx.trace_id.to_string(),
        payload,
    );
    let _ = bus.publish(ctx.clone(), from_ls_event(event)).await;
}

/// 发布会话生命周期事件.
pub async fn emit_session_event(
    bus: &dyn EventBus,
    ctx: &LsContext,
    action: &str, // "created" | "terminated" | "expired"
) {
    let payload = json!({
        "session_id": ctx.session_id.to_string(),
        "action": action,
    });
    let topic = EventTopic::new("runtime", "session", action);
    let event = LsEvent::new(
        topic,
        ctx.session_id.to_string(),
        ctx.trace_id.to_string(),
        payload,
    );
    let _ = bus.publish(ctx.clone(), from_ls_event(event)).await;
}

/// 发布工作流进度事件.
pub async fn emit_workflow_progress(
    bus: &dyn EventBus,
    ctx: &LsContext,
    workflow_name: &str,
    status: &str, // "started" | "completed" | "failed" | "step_progress"
    progress: f64,
    message: Option<&str>,
) {
    let payload = json!({
        "workflow_name": workflow_name,
        "status": status,
        "progress": progress,
        "message": message,
    });
    let event = LsEvent::new(
        EventTopic::new("workflow", "execution", status),
        ctx.session_id.to_string(),
        ctx.trace_id.to_string(),
        payload,
    );
    let _ = bus.publish(ctx.clone(), from_ls_event(event)).await;
}

/// 发布 Runtime 生命周期事件.
pub async fn emit_runtime_event(
    bus: &dyn EventBus,
    ctx: &LsContext,
    action: &str, // "started" | "stopped" | "paused" | "resumed" | "failed"
) {
    let payload = json!({
        "action": action,
    });
    let event = LsEvent::new(
        EventTopic::new("runtime", "runtime", action),
        ctx.session_id.to_string(),
        ctx.trace_id.to_string(),
        payload,
    );
    let _ = bus.publish(ctx.clone(), from_ls_event(event)).await;
}

/// 将 LsEvent 转换为 traits::event_bus::Event.
fn from_ls_event(e: LsEvent) -> lingshu_traits::event_bus::Event {
    lingshu_traits::event_bus::Event {
        event_id: e.event_id,
        topic: e.topic,
        session_id: e.session_id,
        trace_id: e.trace_id,
        payload: e.payload,
        timestamp: e.timestamp,
    }
}
