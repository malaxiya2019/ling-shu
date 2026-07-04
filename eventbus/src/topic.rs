use serde::{Deserialize, Serialize};

/// 事件主题构造器，遵循 `ls.domain.resource.action` 规范.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTopic(String);

impl EventTopic {
    /// 构造主题: `ls.{domain}.{resource}.{action}`
    pub fn new(domain: &str, resource: &str, action: &str) -> Self {
        Self(format!("ls.{}.{}.{}", domain, resource, action))
    }

    /// 从字符串解析，校验格式.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() >= 4 && parts[0] == "ls" {
            Some(Self(s.to_string()))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    // ── 标准生命周期主题 ──

    pub fn runtime_started() -> Self {
        Self::new("runtime", "runtime", "started")
    }

    pub fn session_created() -> Self {
        Self::new("runtime", "session", "created")
    }

    pub fn session_terminated() -> Self {
        Self::new("runtime", "session", "terminated")
    }

    // ── 标准执行调度主题 ──

    pub fn task_submitted() -> Self {
        Self::new("runtime", "task", "submitted")
    }

    pub fn agent_step_finished() -> Self {
        Self::new("agent", "step", "finished")
    }

    pub fn llm_request_sent() -> Self {
        Self::new("llm", "request", "sent")
    }

    // ── 标准能力调用主题 ──

    pub fn tool_called() -> Self {
        Self::new("tool", "call", "called")
    }

    pub fn memory_written() -> Self {
        Self::new("memory", "data", "written")
    }

    pub fn knowledge_synced() -> Self {
        Self::new("knowledge", "source", "synced")
    }

    // ── 标准扩展与故障主题 ──

    pub fn plugin_loaded() -> Self {
        Self::new("plugin", "plugin", "loaded")
    }

    pub fn fault_detected() -> Self {
        Self::new("fault", "fault", "detected")
    }

    pub fn recovery_completed() -> Self {
        Self::new("fault", "recovery", "completed")
    }

    // ── 标准审计合规主题 ──

    pub fn permission_denied() -> Self {
        Self::new("audit", "permission", "denied")
    }

    pub fn quota_exceeded() -> Self {
        Self::new("audit", "quota", "exceeded")
    }

    pub fn config_updated() -> Self {
        Self::new("audit", "config", "updated")
    }
}

impl std::fmt::Display for EventTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_format() {
        let t = EventTopic::new("agent", "run", "completed");
        assert_eq!(t.as_str(), "ls.agent.run.completed");
    }

    #[test]
    fn test_parse_valid() {
        let t = EventTopic::parse("ls.agent.run.completed").unwrap();
        assert_eq!(t.as_str(), "ls.agent.run.completed");
    }

    #[test]
    fn test_parse_invalid() {
        assert!(EventTopic::parse("invalid.topic").is_none());
        assert!(EventTopic::parse("no.ls.prefix").is_none());
    }

    #[test]
    fn test_standard_topics() {
        assert_eq!(EventTopic::runtime_started().as_str(), "ls.runtime.runtime.started");
        assert_eq!(EventTopic::session_created().as_str(), "ls.runtime.session.created");
        assert_eq!(EventTopic::permission_denied().as_str(), "ls.audit.permission.denied");
    }
}
