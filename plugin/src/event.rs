//! 🔔 Plugin Event System — Registrar + EventBus.
//!
//! 借鉴 BeEF API Registrar 模式，为 LingShu 提供插件事件钩子系统。
//! 插件和核心组件可通过 [`Registrar`] 注册回调，在关键生命周期点
//! 通过 [`EventBus`] 发布事件，实现松耦合的事件驱动通信。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

// ── Event Types ─────────────────────────────────────

/// 事件类型.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    // ── 插件生命周期 ──
    /// 插件已注册.
    PluginInstalled,
    /// 插件已初始化 (Loaded).
    PluginLoaded,
    /// 插件已启动 (Running).
    PluginStarted,
    /// 插件已停止 (Stopped).
    PluginStopped,
    /// 插件已卸载.
    PluginUninstalled,
    /// 插件出错.
    PluginFailed(String),

    // ── Agent 生命周期 ──
    /// Agent 已创建.
    AgentCreated,
    /// Agent 已启动执行.
    AgentStarted,
    /// Agent 执行完毕.
    AgentCompleted,
    /// Agent 执行失败.
    AgentFailed(String),

    // ── 消息 ──
    /// 收到消息.
    MessageReceived,
    /// 消息已发送.
    MessageSent,

    // ── 联邦网络 ──
    /// 联邦节点加入.
    FederationNodeJoined,
    /// 联邦节点离开.
    FederationNodeLeft,
    /// 联邦同步完成.
    FederationSyncComplete,

    // ── 插件市场 & 热加载 ──
    /// 从市场安装插件.
    MarketInstall,
    /// 市场源已添加.
    MarketSourceAdded,
    /// 热加载已启动.
    HotReloadStarted,
    /// 热加载已停止.
    HotReloadStopped,
    /// 热加载检测到插件变更.
    HotReloadPluginChanged,
}

// ── Event ───────────────────────────────────────────

/// 事件载荷.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// 事件类型.
    #[serde(rename = "type")]
    pub event_type: EventType,
    /// 事件产生时间.
    pub timestamp: DateTime<Utc>,
    /// 事件源 (e.g. "plugin:my-plugin", "agent:agent-1", "system").
    pub source: String,
    /// 事件数据 (JSON).
    pub payload: serde_json::Value,
}

impl Event {
    /// 创建一个新事件.
    pub fn new(event_type: EventType, source: impl Into<String>, payload: impl Serialize) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type,
            source: source.into(),
            payload: serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
        }
    }
}

// ── Event Handler ───────────────────────────────────

/// 事件回调签名.
pub type EventCallback = Arc<dyn Fn(Event) + Send + Sync + 'static>;

// ── Registrar ───────────────────────────────────────

/// 事件注册器 — 注册/注销事件回调.
///
/// 借鉴 BeEF 的 API Registrar 模式，允许插件和核心组件
/// 为特定 [`EventType`] 注册监听回调。
pub struct Registrar {
    handlers: Arc<RwLock<HashMap<EventType, Vec<HandlerEntry>>>>,
    next_id: AtomicU64,
}

#[allow(dead_code)]
struct HandlerEntry {
    id: u64,
    callback: EventCallback,
    description: String,
}

impl Registrar {
    /// 创建空注册器.
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        }
    }

    /// 注册事件回调，返回 handler ID (可用于注销).
    pub async fn register(
        &self,
        event_type: EventType,
        callback: EventCallback,
        description: impl Into<String>,
    ) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut map = self.handlers.write().await;
        map.entry(event_type).or_default().push(HandlerEntry {
            id,
            callback,
            description: description.into(),
        });
        id
    }

    /// 按 handler ID 注销回调.
    pub async fn unregister(&self, handler_id: u64) -> bool {
        let mut map = self.handlers.write().await;
        for (_, entries) in map.iter_mut() {
            if let Some(pos) = entries.iter().position(|e| e.id == handler_id) {
                entries.swap_remove(pos);
                return true;
            }
        }
        false
    }

    /// 清空指定事件类型的所有回调.
    pub async fn clear(&self, event_type: &EventType) {
        let mut map = self.handlers.write().await;
        map.remove(event_type);
    }

    /// 清空所有回调.
    pub async fn clear_all(&self) {
        let mut map = self.handlers.write().await;
        map.clear();
    }

    /// 获取某事件类型的全部回调.
    pub(crate) async fn callbacks_for(&self, event_type: &EventType) -> Vec<EventCallback> {
        let map = self.handlers.read().await;
        map.get(event_type)
            .map(|entries| entries.iter().map(|e| e.callback.clone()).collect())
            .unwrap_or_default()
    }

    /// 获取注册统计 (每种事件类型有多少回调).
    pub async fn stats(&self) -> HashMap<EventType, usize> {
        let map = self.handlers.read().await;
        map.iter().map(|(k, v)| (k.clone(), v.len())).collect()
    }
}

impl Default for Registrar {
    fn default() -> Self {
        Self::new()
    }
}

// ── EventBus ────────────────────────────────────────

/// 事件总线 — 发布事件到已注册的回调.
///
/// 用法:
/// ```ignore
/// let bus = EventBus::new();
/// let id = bus.registrar().register(
///     EventType::PluginStarted,
///     Arc::new(|event| {
///         tracing::info!("插件启动事件: {:?}", event);
///     }),
///     "log-plugin-events",
/// ).await;
///
/// bus.publish(&Event::new(
///     EventType::PluginStarted,
///     "plugin:hello",
///     serde_json::json!({"name": "hello-plugin", "version": "1.0.0"}),
/// )).await;
/// ```
pub struct EventBus {
    registrar: Arc<Registrar>,
}

impl EventBus {
    /// 创建新的事件总线.
    pub fn new() -> Self {
        Self {
            registrar: Arc::new(Registrar::new()),
        }
    }

    /// 获取注册器的引用.
    pub fn registrar(&self) -> &Arc<Registrar> {
        &self.registrar
    }

    /// 发布事件到所有注册的回调.
    ///
    /// 所有回调在当前任务中顺序同步执行。
    /// 如果回调需要执行异步操作，应自行 `spawn`。
    pub async fn publish(&self, event: &Event) {
        let callbacks = self.registrar.callbacks_for(&event.event_type).await;
        for cb in callbacks {
            // 用 tokio::spawn 包裹，避免回调 panic 影响发布者
            let event_clone = event.clone();
            tokio::spawn(async move {
                cb(event_clone);
            });
        }
    }

    /// 同步发布事件 (用于非 async 上下文).
    pub fn publish_blocking(&self, event: &Event) {
        let event = event.clone();
        let event_type = event.event_type.clone();
        let callbacks = {
            let registrar = self.registrar.clone();
            futures::executor::block_on(async move { registrar.callbacks_for(&event_type).await })
        };
        for cb in callbacks {
            let event_clone = event.clone();
            std::thread::spawn(move || cb(event_clone));
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helper macros ───────────────────────────────────

/// 快速创建事件.
#[macro_export]
macro_rules! emit_event {
    ($bus:expr, $type:expr, $source:expr, $payload:expr) => {
        $bus.publish(&$crate::event::Event::new($type, $source, $payload))
            .await;
    };
    ($bus:expr, $type:expr, $source:expr) => {
        $bus.publish(&$crate::event::Event::new(
            $type,
            $source,
            serde_json::json!({}),
        ))
        .await;
    };
}

// ── Tests ───────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[tokio::test]
    async fn test_register_and_publish() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        bus.registrar()
            .register(
                EventType::PluginStarted,
                Arc::new(move |_event| {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
                "test-counter",
            )
            .await;

        bus.publish(&Event::new(
            EventType::PluginStarted,
            "test",
            serde_json::json!({"key": "value"}),
        ))
        .await;

        // 给 tokio::spawn 一点时间执行回调
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_multi_handler() {
        let bus = EventBus::new();
        let calls = Arc::new(AtomicU32::new(0));
        let c1 = calls.clone();
        let c2 = calls.clone();

        bus.registrar()
            .register(
                EventType::PluginStopped,
                Arc::new(move |_| {
                    c1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
                "h1",
            )
            .await;
        bus.registrar()
            .register(
                EventType::PluginStopped,
                Arc::new(move |_| {
                    c2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
                "h2",
            )
            .await;

        bus.publish(&Event::new(
            EventType::PluginStopped,
            "test",
            serde_json::json!({}),
        ))
        .await;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_unregister() {
        let bus = EventBus::new();
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();

        let id = bus
            .registrar()
            .register(
                EventType::PluginInstalled,
                Arc::new(move |_| {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
                "removable",
            )
            .await;

        bus.publish(&Event::new(
            EventType::PluginInstalled,
            "test",
            serde_json::json!({}),
        ))
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        // 注销后再发布不应触发
        bus.registrar().unregister(id).await;
        bus.publish(&Event::new(
            EventType::PluginInstalled,
            "test",
            serde_json::json!({}),
        ))
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_event_type_dispatch() {
        let bus = EventBus::new();
        let started = Arc::new(AtomicU32::new(0));
        let stopped = Arc::new(AtomicU32::new(0));
        let s = started.clone();
        let t = stopped.clone();

        bus.registrar()
            .register(
                EventType::PluginStarted,
                Arc::new(move |_| {
                    s.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
                "start-counter",
            )
            .await;
        bus.registrar()
            .register(
                EventType::PluginStopped,
                Arc::new(move |_| {
                    t.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
                "stop-counter",
            )
            .await;

        bus.publish(&Event::new(
            EventType::PluginStarted,
            "test",
            serde_json::json!({}),
        ))
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(started.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(stopped.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_stats() {
        let bus = EventBus::new();
        let cb = Arc::new(|_: Event| {});

        bus.registrar()
            .register(EventType::PluginStarted, cb.clone(), "h1")
            .await;
        bus.registrar()
            .register(EventType::PluginStarted, cb.clone(), "h2")
            .await;
        bus.registrar()
            .register(EventType::PluginStopped, cb, "h3")
            .await;

        let stats = bus.registrar().stats().await;
        assert_eq!(stats.get(&EventType::PluginStarted), Some(&2));
        assert_eq!(stats.get(&EventType::PluginStopped), Some(&1));
    }
}
