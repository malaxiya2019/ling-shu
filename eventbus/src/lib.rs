//! LSEventBus — 事件体系实现。
//!
//! 事件主题命名规范: `ls.domain.resource.action`
//! 投递策略: 至少一次投递，消费幂等
//! 失败策略: 最多重试 3 次 → 死信队列

pub mod bus;
pub mod event;
pub mod topic;

pub use bus::InMemoryEventBus;
pub use event::*;
pub use topic::*;
