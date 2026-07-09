//! 📋 ChannelRegistry — 通道插件注册表.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::traits::MessageChannel;

/// 通道插件注册表.
///
/// 支持已加载插件 + 内置插件懒加载两种注册方式.
/// 对应 OpenClaw 的 `getChannelPlugin()` + `listChannelPlugins()`.
pub struct ChannelRegistry {
    /// 已加载插件.
    loaded: RwLock<HashMap<&'static str, Arc<dyn MessageChannel>>>,
    /// 内置插件工厂 (懒加载).
    #[allow(clippy::type_complexity)]
    builtins: RwLock<HashMap<&'static str, Box<dyn Fn() -> Arc<dyn MessageChannel> + Send + Sync>>>,
    /// ID → 别名映射.
    aliases: RwLock<HashMap<String, &'static str>>,
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelRegistry {
    /// 创建空注册表.
    pub fn new() -> Self {
        Self {
            loaded: RwLock::new(HashMap::new()),
            builtins: RwLock::new(HashMap::new()),
            aliases: RwLock::new(HashMap::new()),
        }
    }

    /// 注册一个已初始化的通道插件.
    pub async fn register(&self, channel: Arc<dyn MessageChannel>) {
        let id = channel.id();
        let meta = channel.meta();
        let mut loaded = self.loaded.write().await;
        loaded.insert(id, channel);
        // 注册别名
        let mut aliases = self.aliases.write().await;
        for alias in meta.aliases {
            aliases.insert(alias.to_string(), id);
        }
    }

    /// 注册一个内置插件工厂 (懒加载).
    pub async fn register_builtin(
        &self,
        factory: Box<dyn Fn() -> Arc<dyn MessageChannel> + Send + Sync>,
    ) {
        // 创建临时实例获取 ID
        let temp = factory();
        let id = temp.id();
        let meta = temp.meta().clone();
        drop(temp);
        let mut builtins = self.builtins.write().await;
        builtins.insert(id, factory);
        // 注册别名
        let mut aliases = self.aliases.write().await;
        for alias in meta.aliases {
            aliases.insert(alias.to_string(), id);
        }
    }

    /// 获取通道插件 (已加载 → 内置懒加载回退).
    pub async fn get(&self, id: &str) -> Option<Arc<dyn MessageChannel>> {
        // 1. 检查已加载
        {
            let loaded = self.loaded.read().await;
            if let Some(ch) = loaded.get(id) {
                return Some(ch.clone());
            }
        }
        // 2. 检查内置工厂
        {
            let builtins = self.builtins.read().await;
            if let Some(factory) = builtins.get(id) {
                let ch = factory();
                let id = ch.id();
                let ch = ch;
                // 移动到已加载
                drop(builtins);
                let mut loaded = self.loaded.write().await;
                loaded.insert(id, ch.clone());
                return Some(ch);
            }
        }
        None
    }

    /// 通过别名规范化通道 ID.
    pub async fn normalize_id(&self, raw: &str) -> Option<&'static str> {
        // 直接匹配
        {
            let loaded = self.loaded.read().await;
            if loaded.contains_key(raw) {
                return Some(Box::leak(raw.to_string().into_boxed_str()));
            }
        }
        // 别名匹配
        let aliases = self.aliases.read().await;
        aliases.get(raw).copied()
    }

    /// 列出所有可用通道 ID.
    pub async fn list(&self) -> Vec<&'static str> {
        let loaded = self.loaded.read().await;
        let builtins = self.builtins.read().await;
        let mut ids: Vec<&'static str> = loaded.keys().copied().collect();
        for id in builtins.keys() {
            if !ids.contains(id) {
                ids.push(id);
            }
        }
        ids
    }

    /// 列出所有已加载通道的元数据.
    pub async fn list_meta(&self) -> Vec<(String, crate::types::ChannelMeta)> {
        let loaded = self.loaded.read().await;
        loaded
            .iter()
            .map(|(id, ch)| (id.to_string(), ch.meta()))
            .collect()
    }
}
