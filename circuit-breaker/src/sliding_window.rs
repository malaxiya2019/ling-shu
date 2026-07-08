//! 滑动窗口计数器，用于统计时间窗口内的请求失败次数.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 滑动窗口计数器.
#[derive(Debug, Clone)]
pub struct SlidingWindow {
    /// 窗口大小（秒）.
    window_secs: u64,
    /// 时间戳队列（存储每个事件的发生时间）.
    events: VecDeque<Instant>,
}

impl SlidingWindow {
    /// 创建新的滑动窗口.
    pub fn new(window_secs: u64) -> Self {
        Self {
            window_secs,
            events: VecDeque::new(),
        }
    }

    /// 记录一个事件.
    pub fn record(&mut self) {
        self.events.push_back(Instant::now());
        self.evict_old();
    }

    /// 窗口内的事件总数.
    pub fn count(&mut self) -> u64 {
        self.evict_old();
        self.events.len() as u64
    }

    /// 窗口内的失败次数是否达到阈值.
    pub fn is_threshold_reached(&mut self, threshold: u64) -> bool {
        self.count() >= threshold
    }

    /// 清除所有事件.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// 移除窗口外的事件.
    fn evict_old(&mut self) {
        let cutoff = Instant::now() - Duration::from_secs(self.window_secs);
        while let Some(front) = self.events.front() {
            if *front < cutoff {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_empty_window() {
        let mut sw = SlidingWindow::new(60);
        assert_eq!(sw.count(), 0);
    }

    #[test]
    fn test_record_and_count() {
        let mut sw = SlidingWindow::new(60);
        sw.record();
        sw.record();
        sw.record();
        assert_eq!(sw.count(), 3);
    }

    #[test]
    fn test_threshold() {
        let mut sw = SlidingWindow::new(60);
        for _ in 0..5 {
            sw.record();
        }
        assert!(sw.is_threshold_reached(5));
        assert!(!sw.is_threshold_reached(6));
    }

    #[test]
    fn test_clear() {
        let mut sw = SlidingWindow::new(60);
        sw.record();
        sw.record();
        sw.clear();
        assert_eq!(sw.count(), 0);
    }

    #[test]
    fn test_eviction() {
        let mut sw = SlidingWindow::new(1); // 1-second window
        sw.record();
        thread::sleep(Duration::from_millis(1100));
        assert_eq!(sw.count(), 0); // should be evicted
    }
}
