//! AgentSwarm — 群体性能监控与指标
//!
//! 收集 Swarm 级别的性能指标：
//! - 吞吐量（任务/秒）
//! - 延迟（P50/P90/P99）
//! - 成功率
//! - Agent 利用率
//! - 共识效率

use crate::types::*;
use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use tokio::sync::RwLock;

// ── 指标点 ──────────────────────────────────────────

/// 单个指标数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    /// 时间戳
    pub timestamp: i64,
    /// 指标值
    pub value: f64,
}

/// Swarm 性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmMetrics {
    /// Swarm ID
    pub swarm_id: LsId,
    /// 吞吐量（任务/分钟）
    pub throughput: f64,
    /// 成功率（0.0~1.0）
    pub success_rate: f64,
    /// 平均执行时长 ms
    pub avg_execution_ms: f64,
    /// P50 延迟 ms
    pub p50_latency_ms: f64,
    /// P90 延迟 ms
    pub p90_latency_ms: f64,
    /// P99 延迟 ms
    pub p99_latency_ms: f64,
    /// Agent 利用率（0.0~1.0）
    pub agent_utilization: f64,
    /// 共识达成率（0.0~1.0）
    pub consensus_rate: f64,
    /// 活跃 Agent 数
    pub active_agents: usize,
    /// 总 Agent 数
    pub total_agents: usize,
    /// 指标收集时间
    pub collected_at: i64,
    /// 自定义指标
    pub custom_metrics: HashMap<String, f64>,
}

impl SwarmMetrics {
    pub fn new(swarm_id: LsId) -> Self {
        Self {
            swarm_id,
            throughput: 0.0,
            success_rate: 1.0,
            avg_execution_ms: 0.0,
            p50_latency_ms: 0.0,
            p90_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            agent_utilization: 0.0,
            consensus_rate: 1.0,
            active_agents: 0,
            total_agents: 0,
            collected_at: chrono::Utc::now().timestamp(),
            custom_metrics: HashMap::new(),
        }
    }
}

// ── 指标收集器 ──────────────────────────────────────

/// Swarm 指标收集器
pub struct MetricsCollector {
    /// 延迟历史（用于 P50/P90/P99 计算）
    latency_history: RwLock<VecDeque<f64>>,
    /// 成功/失败计数
    success_count: RwLock<u64>,
    failure_count: RwLock<u64>,
    /// 共识成功/失败计数
    consensus_success: RwLock<u64>,
    consensus_failure: RwLock<u64>,
    /// 时间窗口内的任务计数（用于吞吐量）
    task_timestamps: RwLock<VecDeque<i64>>,
    /// 最大历史记录数
    max_history: usize,
    /// 窗口大小（秒）
    window_secs: i64,
}

impl MetricsCollector {
    pub fn new(max_history: usize, window_secs: i64) -> Self {
        Self {
            latency_history: RwLock::new(VecDeque::with_capacity(max_history)),
            success_count: RwLock::new(0),
            failure_count: RwLock::new(0),
            consensus_success: RwLock::new(0),
            consensus_failure: RwLock::new(0),
            task_timestamps: RwLock::new(VecDeque::new()),
            max_history,
            window_secs,
        }
    }

    /// 记录任务执行
    pub async fn record_execution(&self, success: bool, execution_ms: f64) {
        let now = chrono::Utc::now().timestamp();

        // 更新延迟历史
        let mut latency = self.latency_history.write().await;
        latency.push_back(execution_ms);
        while latency.len() > self.max_history {
            latency.pop_front();
        }

        // 更新成功/失败计数
        if success {
            *self.success_count.write().await += 1;
        } else {
            *self.failure_count.write().await += 1;
        }

        // 更新时间窗口内的任务
        let mut timestamps = self.task_timestamps.write().await;
        timestamps.push_back(now);
        while let Some(&t) = timestamps.front() {
            if now - t > self.window_secs {
                timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// 记录共识结果
    pub async fn record_consensus(&self, achieved: bool) {
        if achieved {
            *self.consensus_success.write().await += 1;
        } else {
            *self.consensus_failure.write().await += 1;
        }
    }

    /// 计算当前指标
    pub async fn metrics(&self, swarm_id: LsId, state: &SwarmState) -> SwarmMetrics {
        let latency = self.latency_history.read().await;
        let success = *self.success_count.read().await;
        let failure = *self.failure_count.read().await;
        let consensus_success = *self.consensus_success.read().await;
        let consensus_failure = *self.consensus_failure.read().await;
        let timestamps = self.task_timestamps.read().await;

        // 计算延迟分位数
        let mut sorted: Vec<f64> = latency.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let p50 = percentile(&sorted, 50.0);
        let p90 = percentile(&sorted, 90.0);
        let p99 = percentile(&sorted, 99.0);

        // 计算吞吐量（任务/分钟 = 窗口内任务数 / 窗口秒数 * 60）
        let window_duration = self.window_secs as f64;
        let throughput = if window_duration > 0.0 {
            timestamps.len() as f64 / window_duration * 60.0
        } else {
            0.0
        };

        // 成功率
        let total = success + failure;
        let success_rate = if total > 0 {
            success as f64 / total as f64
        } else {
            1.0
        };

        // 共识率
        let consensus_total = consensus_success + consensus_failure;
        let consensus_rate = if consensus_total > 0 {
            consensus_success as f64 / consensus_total as f64
        } else {
            1.0
        };

        // Agent 利用率
        let total_agents = state.agent_count();
        let active_agents = state.busy_agent_count() + state.available_agent_count();
        let agent_utilization = if total_agents > 0 {
            state.busy_agent_count() as f64 / total_agents as f64
        } else {
            0.0
        };

        // 平均执行时长
        let avg_execution_ms = if !sorted.is_empty() {
            sorted.iter().sum::<f64>() / sorted.len() as f64
        } else {
            0.0
        };

        SwarmMetrics {
            swarm_id,
            throughput,
            success_rate,
            avg_execution_ms,
            p50_latency_ms: p50,
            p90_latency_ms: p90,
            p99_latency_ms: p99,
            agent_utilization,
            consensus_rate,
            active_agents,
            total_agents,
            collected_at: chrono::Utc::now().timestamp(),
            custom_metrics: HashMap::new(),
        }
    }

    /// 重置所有指标
    pub async fn reset(&self) {
        self.latency_history.write().await.clear();
        *self.success_count.write().await = 0;
        *self.failure_count.write().await = 0;
        *self.consensus_success.write().await = 0;
        *self.consensus_failure.write().await = 0;
        self.task_timestamps.write().await.clear();
    }
}

/// 计算分位数
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let len = sorted.len();
    let k = ((p / 100.0) * (len as f64 - 1.0)).round() as usize;
    sorted[k.min(len - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = MetricsCollector::new(1000, 60);
        for _ in 0..10 {
            collector.record_execution(true, 50.0).await;
            collector.record_execution(true, 100.0).await;
            collector.record_execution(false, 200.0).await;
        }

        collector.record_consensus(true).await;
        collector.record_consensus(true).await;
        collector.record_consensus(false).await;

        let state = SwarmState::new("test", SwarmStrategy::Democratic, SwarmTopology::Mesh);
        let metrics = collector.metrics(LsId::new(), &state).await;

        assert!(metrics.success_rate > 0.0);
        assert!(metrics.consensus_rate > 0.0);
        assert_eq!(metrics.avg_execution_ms, (50.0 + 100.0 + 200.0) / 3.0);
        assert!(metrics.p50_latency_ms >= 50.0);
    }

    #[tokio::test]
    async fn test_metrics_empty() {
        let collector = MetricsCollector::new(1000, 60);
        let state = SwarmState::new("empty", SwarmStrategy::Voting, SwarmTopology::Star);
        let metrics = collector.metrics(LsId::new(), &state).await;
        assert_eq!(metrics.success_rate, 1.0);
        assert_eq!(metrics.avg_execution_ms, 0.0);
        assert_eq!(metrics.p50_latency_ms, 0.0);
    }

    #[tokio::test]
    async fn test_reset() {
        let collector = MetricsCollector::new(1000, 60);
        collector.record_execution(true, 100.0).await;
        collector.reset().await;

        let state = SwarmState::new("reset", SwarmStrategy::Hierarchical, SwarmTopology::Ring);
        let metrics = collector.metrics(LsId::new(), &state).await;
        assert_eq!(metrics.success_rate, 1.0); // No data => defaults to 1.0
        assert_eq!(metrics.agent_utilization, 0.0);
    }

    #[test]
    fn test_percentile() {
        let data = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
        assert_eq!(percentile(&data, 50.0), 60.0); // index round(4.5)=5, sorted[5]=60
        assert_eq!(percentile(&data, 90.0), 90.0); // index round(8.1)=8, sorted[8]=90
        assert_eq!(percentile(&data, 0.0), 10.0);
        assert_eq!(percentile(&data, 100.0), 100.0);
    }

    #[test]
    fn test_percentile_empty() {
        assert_eq!(percentile(&[], 50.0), 0.0);
    }

    #[test]
    fn test_swarm_metrics_new() {
        let metrics = SwarmMetrics::new(LsId::new());
        assert_eq!(metrics.total_agents, 0);
        assert_eq!(metrics.success_rate, 1.0);
    }
}
