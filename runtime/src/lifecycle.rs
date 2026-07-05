use lingshu_core::{LsContext, LsError, LsResult};
use serde::{Deserialize, Serialize};

/// Runtime 生命周期状态机.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleState {
    Uninitialized,
    Initializing,
    Running,
    Pausing,
    Paused,
    ShuttingDown,
    Stopped,
    Failed,
}

impl LifecycleState {
    /// 校验状态转换合法性.
    pub fn can_transition_to(self, next: LifecycleState) -> bool {
        match (self, next) {
            // 正常流程
            (LifecycleState::Uninitialized, LifecycleState::Initializing) => true,
            (LifecycleState::Initializing, LifecycleState::Running) => true,
            (LifecycleState::Initializing, LifecycleState::Failed) => true,
            (LifecycleState::Running, LifecycleState::Pausing) => true,
            (LifecycleState::Pausing, LifecycleState::Paused) => true,
            (LifecycleState::Paused, LifecycleState::Running) => true,
            (LifecycleState::Running, LifecycleState::ShuttingDown) => true,
            (LifecycleState::Paused, LifecycleState::ShuttingDown) => true,
            (LifecycleState::ShuttingDown, LifecycleState::Stopped) => true,
            (LifecycleState::ShuttingDown, LifecycleState::Failed) => true,
            // 任意状态可进入 Failed
            (_, LifecycleState::Failed) => true,
            // 从 Failed 只能到 Stopped
            (LifecycleState::Failed, LifecycleState::Stopped) => true,
            _ => false,
        }
    }

    /// 是否正在运行.
    pub fn is_running(self) -> bool {
        matches!(self, LifecycleState::Running)
    }

    /// 是否已停止.
    pub fn is_stopped(self) -> bool {
        matches!(self, LifecycleState::Stopped | LifecycleState::Failed)
    }

    /// 是否可接受新任务.
    pub fn can_accept_tasks(self) -> bool {
        matches!(self, LifecycleState::Running)
    }
}

/// 生命周期管理器.
#[derive(Debug)]
pub struct LifecycleManager {
    state: std::sync::RwLock<LifecycleState>,
}

impl LifecycleManager {
    pub fn new() -> Self {
        Self {
            state: std::sync::RwLock::new(LifecycleState::Uninitialized),
        }
    }

    /// 尝试状态转换，失败返回错误.
    pub fn transition(&self, ctx: &LsContext, next: LifecycleState) -> LsResult<LifecycleState> {
        let mut state = self
            .state
            .write()
            .map_err(|e| LsError::Internal(format!("lifecycle lock poisoned: {e}")))?;
        let current = *state;
        if !state.can_transition_to(next) {
            tracing::warn!(
                trace_id = %ctx.trace_id,
                from = ?current,
                to = ?next,
                "lifecycle invalid transition"
            );
            return Err(LsError::RuntimeState(format!(
                "cannot transition from {current:?} to {next:?}"
            )));
        }
        *state = next;
        tracing::info!(
            trace_id = %ctx.trace_id,
            from = ?current,
            to = ?next,
            "lifecycle transition"
        );
        Ok(next)
    }

    /// 读取当前状态.
    pub fn current(&self) -> LsResult<LifecycleState> {
        self.state
            .read()
            .map(|s| *s)
            .map_err(|e| LsError::Internal(format!("lifecycle lock poisoned: {e}")))
    }

    /// 是否已就绪.
    pub fn is_ready(&self) -> bool {
        self.current().map(|s| s.is_running()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mgr = LifecycleManager::new();
        assert_eq!(
            mgr.transition(&ctx, LifecycleState::Initializing).unwrap(),
            LifecycleState::Initializing
        );
        assert_eq!(
            mgr.transition(&ctx, LifecycleState::Running).unwrap(),
            LifecycleState::Running
        );
        assert!(mgr.transition(&ctx, LifecycleState::Uninitialized).is_err());
    }

    #[test]
    fn test_cannot_skip_states() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mgr = LifecycleManager::new();
        assert!(mgr.transition(&ctx, LifecycleState::Running).is_err());
    }

    #[test]
    fn test_full_lifecycle() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mgr = LifecycleManager::new();
        assert_eq!(mgr.current().unwrap(), LifecycleState::Uninitialized);
        assert!(!mgr.is_ready());

        mgr.transition(&ctx, LifecycleState::Initializing).unwrap();
        mgr.transition(&ctx, LifecycleState::Running).unwrap();
        assert!(mgr.is_ready());
        assert!(mgr.current().unwrap().can_accept_tasks());

        mgr.transition(&ctx, LifecycleState::ShuttingDown).unwrap();
        assert!(!mgr.current().unwrap().can_accept_tasks());
        mgr.transition(&ctx, LifecycleState::Stopped).unwrap();
        assert!(mgr.current().unwrap().is_stopped());
    }

    #[test]
    fn test_failed_state_recovery() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mgr = LifecycleManager::new();
        mgr.transition(&ctx, LifecycleState::Initializing).unwrap();
        mgr.transition(&ctx, LifecycleState::Failed).unwrap();
        assert!(mgr.current().unwrap().is_stopped());
        mgr.transition(&ctx, LifecycleState::Stopped).unwrap();
    }

    #[test]
    fn test_pause_resume() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mgr = LifecycleManager::new();
        mgr.transition(&ctx, LifecycleState::Initializing).unwrap();
        mgr.transition(&ctx, LifecycleState::Running).unwrap();
        mgr.transition(&ctx, LifecycleState::Pausing).unwrap();
        mgr.transition(&ctx, LifecycleState::Paused).unwrap();
        assert!(!mgr.current().unwrap().can_accept_tasks());
        mgr.transition(&ctx, LifecycleState::Running).unwrap();
        assert!(mgr.is_ready());
    }

    #[test]
    fn test_can_transition_to_helpers() {
        assert!(LifecycleState::Uninitialized.can_transition_to(LifecycleState::Initializing));
        assert!(!LifecycleState::Uninitialized.can_transition_to(LifecycleState::Running));
        assert!(!LifecycleState::Uninitialized.can_transition_to(LifecycleState::Stopped));
        assert!(LifecycleState::Running.can_transition_to(LifecycleState::Failed));
        assert!(!LifecycleState::Stopped.can_transition_to(LifecycleState::Running));
    }
}
