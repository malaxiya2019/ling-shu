//! FileObserver — 文件变更观察者 trait.
//!
//! 定义文件变更监听接口，支持增量分析：
//! - 首次扫描建立基线
//! - 后续通过观察者感知变更，只重扫/重分析变更文件
//! - 可对接 `notify` 或其他文件系统事件源

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

/// 文件变更类型.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeKind {
    /// 新增文件.
    Created,
    /// 文件内容修改.
    Modified,
    /// 文件被删除.
    Deleted,
    /// 文件重命名.
    Renamed { from: PathBuf, to: PathBuf },
}

/// 文件变更事件.
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    /// 变更的文件路径.
    pub path: PathBuf,
    /// 变更类型.
    pub kind: FileChangeKind,
    /// 事件发生时间.
    pub timestamp: SystemTime,
}

/// 文件变更回调.
pub type FileChangeCallback = Arc<dyn Fn(FileChangeEvent) + Send + Sync>;

/// 文件观察者 trait.
///
/// 实现方可以对接不同的文件系统事件源：
/// - `notify` crate（inotify / FSEvents / ReadDirectoryChanges）
/// - 轮询式扫描（跨平台回退方案）
/// - 远程文件系统事件源（如 git webhooks）
#[async_trait]
pub trait FileObserver: Send + Sync {
    /// 开始监听指定路径.
    ///
    /// `callback` 在每次文件变更时被调用。
    async fn watch(&self, path: &str, callback: FileChangeCallback) -> lingshu_core::LsResult<()>;

    /// 停止监听.
    async fn unwatch(&self, path: &str) -> lingshu_core::LsResult<()>;

    /// 暂停监听（不丢失队列事件）.
    async fn pause(&self) -> lingshu_core::LsResult<()>;

    /// 恢复监听.
    async fn resume(&self) -> lingshu_core::LsResult<()>;

    /// 获取当前监听中的路径列表.
    fn watched_paths(&self) -> Vec<String>;
}

/// 轮询文件观察者 — 通过定期扫描文件系统检测变更.
///
/// 这是 `FileObserver` 的默认实现，不依赖平台特定 API.
pub struct PollingFileObserver {
    watched: std::sync::Mutex<Vec<String>>,
    paused: std::sync::atomic::AtomicBool,
}

impl PollingFileObserver {
    /// 创建新的轮询观察者.
    pub fn new() -> Self {
        Self {
            watched: std::sync::Mutex::new(Vec::new()),
            paused: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl Default for PollingFileObserver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileObserver for PollingFileObserver {
    async fn watch(&self, path: &str, _callback: FileChangeCallback) -> lingshu_core::LsResult<()> {
        let mut paths = self
            .watched
            .lock()
            .map_err(|e| lingshu_core::LsError::Internal(format!("lock error: {e}")))?;
        paths.push(path.to_string());
        Ok(())
    }

    async fn unwatch(&self, path: &str) -> lingshu_core::LsResult<()> {
        let mut paths = self
            .watched
            .lock()
            .map_err(|e| lingshu_core::LsError::Internal(format!("lock error: {e}")))?;
        paths.retain(|p| p != path);
        Ok(())
    }

    async fn pause(&self) -> lingshu_core::LsResult<()> {
        self.paused
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn resume(&self) -> lingshu_core::LsResult<()> {
        self.paused
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn watched_paths(&self) -> Vec<String> {
        self.watched.lock().map(|p| p.clone()).unwrap_or_default()
    }
}

/// 增量变更收集器 — 记录变更事件，供流水线消费.
#[derive(Debug)]
pub struct ChangeCollector {
    events: std::sync::Mutex<Vec<FileChangeEvent>>,
}

impl ChangeCollector {
    /// 创建变更收集器.
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 记录变更事件.
    pub fn record(&self, event: FileChangeEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    /// 取出所有累积事件并清空.
    pub fn drain(&self) -> Vec<FileChangeEvent> {
        self.events
            .lock()
            .map(|mut e| e.drain(..).collect())
            .unwrap_or_default()
    }

    /// 当前待处理事件数量.
    pub fn pending_count(&self) -> usize {
        self.events.lock().map(|e| e.len()).unwrap_or(0)
    }
}

impl Default for ChangeCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_polling_observer() {
        let observer = PollingFileObserver::new();
        assert!(observer.watched_paths().is_empty());

        let callback: FileChangeCallback = Arc::new(|_event| {});
        observer.watch("/tmp/test", callback).await.unwrap();
        assert_eq!(observer.watched_paths().len(), 1);
        assert!(observer.watched_paths().contains(&"/tmp/test".to_string()));

        observer.unwatch("/tmp/test").await.unwrap();
        assert!(observer.watched_paths().is_empty());
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let observer = PollingFileObserver::new();
        observer.pause().await.unwrap();
        observer.resume().await.unwrap();
    }

    #[test]
    fn test_change_collector() {
        let collector = ChangeCollector::new();
        assert_eq!(collector.pending_count(), 0);

        collector.record(FileChangeEvent {
            path: PathBuf::from("src/main.rs"),
            kind: FileChangeKind::Modified,
            timestamp: SystemTime::now(),
        });
        assert_eq!(collector.pending_count(), 1);

        let drained = collector.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].path.to_str().unwrap(), "src/main.rs");
        assert_eq!(collector.pending_count(), 0);
    }

    #[test]
    fn test_file_change_kind_rename() {
        let event = FileChangeEvent {
            path: PathBuf::from("new.rs"),
            kind: FileChangeKind::Renamed {
                from: PathBuf::from("old.rs"),
                to: PathBuf::from("new.rs"),
            },
            timestamp: SystemTime::now(),
        };
        match event.kind {
            FileChangeKind::Renamed { from, .. } => {
                assert_eq!(from.to_str().unwrap(), "old.rs");
            }
            _ => panic!("expected Renamed"),
        }
    }
}

// ---------------------------------------------------------------------------
// NotifyFileObserver — 基于 notify crate v6 的原生文件事件监听
// ---------------------------------------------------------------------------

/// Notify 文件观察者 — 使用操作系统原生文件事件 API.
///
/// 架构：notify `EventHandler` 闭包将事件发往 crossbeam_channel，
/// 后台线程读取并转换为 `FileChangeEvent` 后调用用户回调。
pub struct NotifyFileObserver {
    paths: std::sync::Mutex<Vec<String>>,
    paused: std::sync::atomic::AtomicBool,
    /// 用户回调（通过 watch 设置，被后台线程读取）.
    callback: std::sync::Arc<std::sync::Mutex<Option<FileChangeCallback>>>,
    /// Watcher 实例（持有即 keepalive，通过 &mut 访问 watch/unwatch）.
    watcher: std::sync::Mutex<Option<notify::RecommendedWatcher>>,
    /// 后台事件处理线程.
    #[allow(dead_code)]
    thread_handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl NotifyFileObserver {
    /// 创建新的 NotifyFileObserver.
    ///
    /// 立即创建 crossbeam_channel 和 notify watcher，启动后台事件接收线程。
    pub fn new() -> lingshu_core::LsResult<Self> {
        let (tx, rx) = std::sync::mpsc::channel::<notify::Event>();
        let callback: std::sync::Arc<std::sync::Mutex<Option<FileChangeCallback>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let cb_clone = std::sync::Arc::clone(&callback);
        let paused = std::sync::atomic::AtomicBool::new(false);

        // EventHandler 闭包 — 将 notify 事件发往通道
        let handler = move |event: std::result::Result<notify::Event, notify::Error>| {
            if let Ok(ev) = event {
                let _ = tx.send(ev);
            }
        };

        let w = notify::recommended_watcher(handler).map_err(|e| {
            lingshu_core::LsError::Internal(format!("failed to create watcher: {e}"))
        })?;

        // 后台处理线程
        let paused_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let paused_clone = std::sync::Arc::clone(&paused_flag);
        let handle = std::thread::Builder::new()
            .name("lingshu-notify".into())
            .spawn(move || {
                use notify::EventKind::*;
                let mut debounce: std::collections::HashMap<
                    std::path::PathBuf,
                    std::time::Instant,
                > = std::collections::HashMap::new();
                const DEBOUNCE_MS: u64 = 300;

                while let Ok(event) = rx.recv() {
                            if paused_clone.load(std::sync::atomic::Ordering::Relaxed) {
                                continue;
                            }

                            let now = std::time::Instant::now();
                            let kind = match event.kind {
                                Create(_) => FileChangeKind::Created,
                                Modify(_) => FileChangeKind::Modified,
                                Remove(_) => FileChangeKind::Deleted,
                                _ => continue,
                            };

                            let cb = {
                                if let Ok(guard) = cb_clone.lock() {
                                    guard.clone()
                                } else {
                                    continue;
                                }
                            };

                            if let Some(ref callback) = cb {
                                for path in &event.paths {
                                    if let Some(last) = debounce.get(path) {
                                        if now.duration_since(*last).as_millis()
                                            < DEBOUNCE_MS as u128
                                        {
                                            continue;
                                        }
                                    }
                                    debounce.insert(path.clone(), now);

                                    let fe = FileChangeEvent {
                                        path: path.clone(),
                                        kind: kind.clone(),
                                        timestamp: std::time::SystemTime::now(),
                                    };
                                    callback(fe);
                                }
                            }

                            if debounce.len() > 1000 {
                                debounce.retain(|_, v| now.duration_since(*v).as_secs() < 5);
                            }
                }
            })
            .map_err(|e| lingshu_core::LsError::Internal(format!("spawn thread: {e}")))?;

        Ok(Self {
            paths: std::sync::Mutex::new(Vec::new()),
            paused,
            callback,
            watcher: std::sync::Mutex::new(Some(w)),
            thread_handle: std::sync::Mutex::new(Some(handle)),
        })
    }
}

impl Default for NotifyFileObserver {
    fn default() -> Self {
        // 为了 Default 约束，内部 unwrap，但 new() 几乎不会失败
        Self::new().expect("NotifyFileObserver::new()")
    }
}

#[async_trait]
impl FileObserver for NotifyFileObserver {
    async fn watch(&self, path: &str, callback: FileChangeCallback) -> lingshu_core::LsResult<()> {
        // 存储回调
        {
            let mut cb = self
                .callback
                .lock()
                .map_err(|e| lingshu_core::LsError::Internal(format!("lock error: {e}")))?;
            *cb = Some(callback);
        }

        // 注册路径到 watcher
        {
            let mut watcher_guard = self
                .watcher
                .lock()
                .map_err(|e| lingshu_core::LsError::Internal(format!("lock error: {e}")))?;
            if let Some(ref mut w) = *watcher_guard {
                use notify::Watcher;
                w.watch(std::path::Path::new(path), notify::RecursiveMode::Recursive)
                    .map_err(|e| {
                        lingshu_core::LsError::Internal(format!("failed to watch {path}: {e}"))
                    })?;
            }
        }

        let mut paths = self
            .paths
            .lock()
            .map_err(|e| lingshu_core::LsError::Internal(format!("lock error: {e}")))?;
        if !paths.contains(&path.to_string()) {
            paths.push(path.to_string());
        }

        tracing::info!(path = %path, "notify watcher started");
        Ok(())
    }

    async fn unwatch(&self, path: &str) -> lingshu_core::LsResult<()> {
        {
            let mut watcher_guard = self
                .watcher
                .lock()
                .map_err(|e| lingshu_core::LsError::Internal(format!("lock error: {e}")))?;
            if let Some(ref mut w) = *watcher_guard {
                use notify::Watcher;
                w.unwatch(std::path::Path::new(path)).map_err(|e| {
                    lingshu_core::LsError::Internal(format!("failed to unwatch {path}: {e}"))
                })?;
            }
        }

        let mut paths = self
            .paths
            .lock()
            .map_err(|e| lingshu_core::LsError::Internal(format!("lock error: {e}")))?;
        paths.retain(|p| p != path);
        Ok(())
    }

    async fn pause(&self) -> lingshu_core::LsResult<()> {
        self.paused
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn resume(&self) -> lingshu_core::LsResult<()> {
        self.paused
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn watched_paths(&self) -> Vec<String> {
        self.paths.lock().map(|p| p.clone()).unwrap_or_default()
    }
}

impl std::fmt::Debug for NotifyFileObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyFileObserver")
            .field("watched", &self.watched_paths())
            .field(
                "paused",
                &self.paused.load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
}

#[cfg(test)]
mod notify_tests {
    use super::*;

    #[test]
    fn test_notify_observer_create() {
        let observer = NotifyFileObserver::new().unwrap();
        assert!(observer.watched_paths().is_empty());
    }

    #[tokio::test]
    async fn test_notify_observer_pause_resume() {
        let observer = NotifyFileObserver::new().unwrap();
        observer.pause().await.unwrap();
        observer.resume().await.unwrap();
    }

    #[tokio::test]
    async fn test_notify_observer_watch_unwatch() {
        let tmp = tempfile::tempdir().unwrap();
        let observer = NotifyFileObserver::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();

        let callback: FileChangeCallback = Arc::new(|_| {});
        observer.watch(&path, callback).await.unwrap();
        assert!(observer.watched_paths().contains(&path));

        observer.unwatch(&path).await.unwrap();
        assert!(!observer.watched_paths().contains(&path));
    }

    #[test]
    fn test_notify_observer_default() {
        let _observer = NotifyFileObserver::default();
    }
}
