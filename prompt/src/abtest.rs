//! A/B 测试管理 — 支持提示词版本的灰度发布.

use chrono::{DateTime, Utc};
use lingshu_core::{LsError, LsResult};
use rand::Rng;
use serde::{Deserialize, Serialize};

/// A/B 测试配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTestConfig {
    /// 测试名称.
    pub name: String,
    /// A 版本名称.
    pub variant_a: String,
    /// B 版本名称.
    pub variant_b: String,
    /// B 版本的流量百分比 (0-100).
    pub traffic_percent_b: u8,
    /// 是否启用.
    pub enabled: bool,
    /// 开始时间.
    pub start_at: DateTime<Utc>,
    /// 结束时间（可选）.
    pub end_at: Option<DateTime<Utc>>,
}

/// A/B 测试结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTestResult {
    pub test_name: String,
    pub variant_a: String,
    pub variant_b: String,
    pub a_count: u64,
    pub b_count: u64,
    pub a_success_count: u64,
    pub b_success_count: u64,
    pub a_avg_latency_ms: f64,
    pub b_avg_latency_ms: f64,
}

impl ABTestResult {
    /// A 版本成功率.
    pub fn a_success_rate(&self) -> f64 {
        if self.a_count == 0 {
            return 0.0;
        }
        self.a_success_count as f64 / self.a_count as f64
    }

    /// B 版本成功率.
    pub fn b_success_rate(&self) -> f64 {
        if self.b_count == 0 {
            return 0.0;
        }
        self.b_success_count as f64 / self.b_count as f64
    }

    /// B 版本是否显著更优.
    pub fn b_is_winner(&self) -> bool {
        self.b_success_rate() > self.a_success_rate()
    }
}

/// A/B 测试管理器.
pub struct ABTestManager {
    configs: std::collections::HashMap<String, ABTestConfig>,
    /// 统计 (test_name -> (a_count, b_count, a_success, b_success, a_latency_sum, b_latency_sum))
    stats: std::collections::HashMap<String, (u64, u64, u64, u64, f64, f64)>,
}

impl ABTestManager {
    pub fn new() -> Self {
        Self {
            configs: std::collections::HashMap::new(),
            stats: std::collections::HashMap::new(),
        }
    }

    /// 注册 A/B 测试.
    pub fn register(&mut self, config: ABTestConfig) -> LsResult<()> {
        if self.configs.contains_key(&config.name) {
            return Err(LsError::Internal(format!(
                "AB test '{}' already exists",
                config.name
            )));
        }
        if config.traffic_percent_b > 100 {
            return Err(LsError::Internal(
                "traffic_percent_b must be 0-100".into(),
            ));
        }
        let name = config.name.clone();
        self.configs.insert(name.clone(), config);
        self.stats.insert(name, (0, 0, 0, 0, 0.0, 0.0));
        Ok(())
    }

    /// 选择变体（根据流量分配）.
    pub fn select_variant(&self, test_name: &str) -> LsResult<String> {
        let config = self.configs.get(test_name).ok_or_else(|| {
            LsError::NotFound(format!("AB test '{test_name}' not found"))
        })?;

        if !config.enabled {
            return Ok(config.variant_a.clone());
        }

        if let Some(ref end_at) = config.end_at {
            if Utc::now() > *end_at {
                return Ok(config.variant_a.clone());
            }
        }

        let mut rng = rand::thread_rng();
        let roll: u8 = rng.gen_range(0..100);

        if roll < config.traffic_percent_b {
            Ok(config.variant_b.clone())
        } else {
            Ok(config.variant_a.clone())
        }
    }

    /// 记录一次测试结果.
    pub fn record_result(
        &mut self,
        test_name: &str,
        variant: &str,
        success: bool,
        latency_ms: f64,
    ) -> LsResult<()> {
        let config = self.configs.get(test_name).ok_or_else(|| {
            LsError::NotFound(format!("AB test '{test_name}' not found"))
        })?;

        let stats = self.stats.get_mut(test_name).ok_or_else(|| {
            LsError::NotFound(format!("AB test '{test_name}' stats not found"))
        })?;

        if variant == config.variant_a {
            stats.0 += 1;
            if success {
                stats.2 += 1;
            }
            stats.4 += latency_ms;
        } else if variant == config.variant_b {
            stats.1 += 1;
            if success {
                stats.3 += 1;
            }
            stats.5 += latency_ms;
        }

        Ok(())
    }

    /// 获取测试结果.
    pub fn get_result(&self, test_name: &str) -> LsResult<ABTestResult> {
        let config = self.configs.get(test_name).ok_or_else(|| {
            LsError::NotFound(format!("AB test '{test_name}' not found"))
        })?;

        let stats = self.stats.get(test_name).ok_or_else(|| {
            LsError::NotFound(format!("AB test '{test_name}' stats not found"))
        })?;

        Ok(ABTestResult {
            test_name: test_name.to_string(),
            variant_a: config.variant_a.clone(),
            variant_b: config.variant_b.clone(),
            a_count: stats.0,
            b_count: stats.1,
            a_success_count: stats.2,
            b_success_count: stats.3,
            a_avg_latency_ms: if stats.0 > 0 {
                stats.4 / stats.0 as f64
            } else {
                0.0
            },
            b_avg_latency_ms: if stats.1 > 0 {
                stats.5 / stats.1 as f64
            } else {
                0.0
            },
        })
    }
}

impl Default for ABTestManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(name: &str, percent_b: u8) -> ABTestConfig {
        ABTestConfig {
            name: name.to_string(),
            variant_a: "control".into(),
            variant_b: "experiment".into(),
            traffic_percent_b: percent_b,
            enabled: true,
            start_at: Utc::now(),
            end_at: None,
        }
    }

    #[test]
    fn test_register_and_disable() {
        let mut manager = ABTestManager::new();
        let config = make_config("test1", 50);
        manager.register(config).unwrap();

        // 禁用的测试始终返回 A
        manager.configs.get_mut("test1").unwrap().enabled = false;
        let variant = manager.select_variant("test1").unwrap();
        assert_eq!(variant, "control");
    }

    #[test]
    fn test_traffic_distribution() {
        let mut manager = ABTestManager::new();
        let config = make_config("dist", 100); // 100% B
        manager.register(config).unwrap();

        let mut b_count = 0;
        for _ in 0..1000 {
            let variant = manager.select_variant("dist").unwrap();
            if variant == "experiment" {
                b_count += 1;
            }
        }
        // 100% B，所以全都应该是 experiment
        assert_eq!(b_count, 1000);
    }

    #[test]
    fn test_record_and_get_result() {
        let mut manager = ABTestManager::new();
        let config = make_config("perf", 50);
        manager.register(config).unwrap();

        manager
            .record_result("perf", "control", true, 100.0)
            .unwrap();
        manager
            .record_result("perf", "experiment", true, 80.0)
            .unwrap();
        manager
            .record_result("perf", "experiment", false, 90.0)
            .unwrap();

        let result = manager.get_result("perf").unwrap();
        assert_eq!(result.a_count, 1);
        assert_eq!(result.b_count, 2);
        assert!((result.a_success_rate() - 1.0).abs() < 1e-10);
        assert!((result.b_success_rate() - 0.5).abs() < 1e-10);
    }
}
