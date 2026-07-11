//! CronScheduler — 定时任务调度器.
//!
//! 基于标准 cron 表达式进行任务调度，支持：
//! - 标准 5 字段 cron 表达式 (`min hour dom mon dow`)
//! - 带秒的 6 字段表达式 (`sec min hour dom mon dow`)
//! - @every 语法 (`@every 30s`, `@every 5m`, `@every 1h`)
//! - 预设别名 (`@yearly`, `@monthly`, `@weekly`, `@daily`, `@hourly`)
//!
//! # 示例
//!
//! ```rust,ignore
//! use lingshu_runtime::cron::{CronSchedule, CronManager};
//!
//! let schedule = CronSchedule::parse("*/5 * * * *").unwrap();  // 每 5 分钟
//! let next = schedule.next_occurrence().unwrap();
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Datelike, TimeDelta, Timelike, Utc};
use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;

use crate::task_scheduler::{Job, TaskScheduler};

// ═══════════════════════════════════════════════════════════
// CronSchedule — cron 表达式解析与计算
// ═══════════════════════════════════════════════════════════

/// Cron 调度表达式.
///
/// 支持格式：
/// - 标准 5 字段: `min hour dom mon dow`
/// - 6 字段 (带秒): `sec min hour dom mon dow`
/// - 特殊: `@every <duration>`, `@yearly`, `@monthly`, `@weekly`, `@daily`, `@hourly`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronSchedule {
    expression: String,
    kind: CronKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CronKind {
    /// 5 字段: min hour dom mon dow
    Standard {
        minutes: Vec<u8>,
        hours: Vec<u8>,
        days_of_month: Vec<u8>,
        months: Vec<u8>,
        days_of_week: Vec<u8>,
    },
    /// 6 字段: sec min hour dom mon dow
    WithSeconds {
        seconds: Vec<u8>,
        minutes: Vec<u8>,
        hours: Vec<u8>,
        days_of_month: Vec<u8>,
        months: Vec<u8>,
        days_of_week: Vec<u8>,
    },
    /// 固定间隔: @every 30s, @every 5m, @every 1h
    Every {
        duration_secs: u64,
    },
    /// 预设别名
    Preset(Preset),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Preset {
    Yearly,  // 0 0 1 1 *
    Monthly, // 0 0 1 * *
    Weekly,  // 0 0 * * 0
    Daily,   // 0 0 * * *
    Hourly,  // 0 * * * *
}

impl CronSchedule {
    /// 解析 cron 表达式.
    pub fn parse(expr: &str) -> LsResult<Self> {
        let trimmed = expr.trim().to_lowercase();

        // 预设别名
        let kind = match trimmed.as_str() {
            "@yearly" | "@annually" => CronKind::Preset(Preset::Yearly),
            "@monthly" => CronKind::Preset(Preset::Monthly),
            "@weekly" => CronKind::Preset(Preset::Weekly),
            "@daily" | "@midnight" => CronKind::Preset(Preset::Daily),
            "@hourly" => CronKind::Preset(Preset::Hourly),
            _ => {
                if trimmed.starts_with("@every ") {
                    let dur_str = trimmed.trim_start_matches("@every ");
                    let dur = parse_duration(dur_str)
                        .map_err(|e| LsError::InvalidArgument(format!("invalid @every duration: {e}")))?;
                    CronKind::Every { duration_secs: dur.as_secs() }
                } else {
                    // 标准 cron 表达式
                    let fields: Vec<&str> = trimmed.split_whitespace().collect();
                    match fields.len() {
                        5 => CronKind::Standard {
                            minutes: parse_field(fields[0], 0, 59)?,
                            hours: parse_field(fields[1], 0, 23)?,
                            days_of_month: parse_field(fields[2], 1, 31)?,
                            months: parse_field(fields[3], 1, 12)?,
                            days_of_week: parse_field(fields[4], 0, 6)?,
                        },
                        6 => CronKind::WithSeconds {
                            seconds: parse_field(fields[0], 0, 59)?,
                            minutes: parse_field(fields[1], 0, 59)?,
                            hours: parse_field(fields[2], 0, 23)?,
                            days_of_month: parse_field(fields[3], 1, 31)?,
                            months: parse_field(fields[4], 1, 12)?,
                            days_of_week: parse_field(fields[5], 0, 6)?,
                        },
                        _ => return Err(LsError::InvalidArgument(
                            format!("invalid cron expression: expected 5 or 6 fields, got {}", fields.len())
                        )),
                    }
                }
            }
        };

        Ok(Self {
            expression: expr.to_string(),
            kind,
        })
    }

    /// 计算自给定时间起的下一次触发时间.
    pub fn next_occurrence(&self, from: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
        let now = from.unwrap_or_else(Utc::now);

        match &self.kind {
            CronKind::Every { duration_secs } => {
                let dur = Duration::from_secs(*duration_secs);
                let dur_chrono = TimeDelta::from_std(dur).ok()?;
                Some(now + dur_chrono)
            }
            CronKind::Preset(p) => {
                // 简化: 返回基于当前时间的 next occurrence
                let (min, hour, dom, month, dow) = match p {
                    Preset::Yearly => (0, 0, 1, 1, -1),
                    Preset::Monthly => (0, 0, 1, -1, -1),
                    Preset::Weekly => (0, 0, -1, -1, 0),
                    Preset::Daily => (0, 0, -1, -1, -1),
                    Preset::Hourly => (0, -1, -1, -1, -1),
                };
                find_next_match(now, min, hour, dom, month, dow)
            }
            CronKind::Standard { minutes, hours, days_of_month, months, days_of_week } => {
                find_next_match_multi(now, &[0], minutes, hours, days_of_month, months, days_of_week)
            }
            CronKind::WithSeconds { seconds, minutes, hours, days_of_month, months, days_of_week } => {
                find_next_match_multi(now, seconds, minutes, hours, days_of_month, months, days_of_week)
            }
        }
    }

    /// 获取原始表达式.
    pub fn expression(&self) -> &str {
        &self.expression
    }
}

// ── 辅助函数 ──

/// 解析 cron 字段 (支持 `*`, `*/n`, `n`, `n-m`, `n,m,o`).
fn parse_field(field: &str, min: u8, max: u8) -> LsResult<Vec<u8>> {
    let field = field.trim();
    if field == "*" {
        return Ok((min..=max).collect());
    }

    // 尝试逗号分隔的列表
    let parts: Vec<&str> = field.split(',').collect();
    if parts.len() > 1 {
        let mut result = Vec::new();
        for part in parts {
            result.extend(parse_field(part, min, max)?);
        }
        result.sort();
        result.dedup();
        return Ok(result);
    }

    // step 语法: */n 或 n-m/n
    if let Some((range_part, step_part)) = field.split_once('/') {
        let step: u8 = step_part.parse().map_err(|_| {
            LsError::InvalidArgument(format!("invalid step '{step_part}'"))
        })?;
        let (range_min, range_max) = if range_part == "*" {
            (min, max)
        } else if let Some((a, b)) = range_part.split_once('-') {
            (a.parse::<u8>().unwrap_or(min), b.parse::<u8>().unwrap_or(max))
        } else {
            let v: u8 = range_part.parse().unwrap_or(min);
            (v, max)
        };
        let range_min = range_min.max(min);
        let range_max = range_max.min(max);
        Ok((range_min..=range_max).step_by(step as usize).collect())
    } else if let Some((a, b)) = field.split_once('-') {
        // 范围: n-m
        let a: u8 = a.parse().map_err(|_| {
            LsError::InvalidArgument(format!("invalid range start '{a}'"))
        })?;
        let b: u8 = b.parse().map_err(|_| {
            LsError::InvalidArgument(format!("invalid range end '{b}'"))
        })?;
        Ok((a.min(b)..=a.max(b)).collect())
    } else {
        // 单个值
        let v: u8 = field.parse().map_err(|_| {
            LsError::InvalidArgument(format!("invalid cron value '{field}'"))
        })?;
        if v < min || v > max {
            return Err(LsError::InvalidArgument(
                format!("value {v} out of range [{min}, {max}]")
            ));
        }
        Ok(vec![v])
    }
}

/// 解析人类可读持续时间字符串 (如 "30s", "5m", "1h", "1h30m").
fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration".into());
    }

    let mut total_secs = 0u64;
    let mut current = String::new();

    for ch in s.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else {
            let value: f64 = current.parse().map_err(|_| format!("invalid number '{current}'"))?;
            match ch {
                's' => total_secs += value as u64,
                'm' => total_secs += (value * 60.0) as u64,
                'h' => total_secs += (value * 3600.0) as u64,
                'd' => total_secs += (value * 86400.0) as u64,
                _ => return Err(format!("unknown unit '{ch}'")),
            }
            current.clear();
        }
    }

    if !current.is_empty() {
        // 无单位后缀，视为秒
        total_secs += current.parse::<u64>().map_err(|_| format!("invalid number '{current}'"))?;
    }

    Ok(Duration::from_secs(total_secs))
}

/// 查找下一个匹配的时间点 (单个值匹配).
#[allow(clippy::too_many_arguments)]
fn find_next_match(
    now: DateTime<Utc>,
    minute: i32,
    hour: i32,
    day: i32,
    month: i32,
    weekday: i32,
) -> Option<DateTime<Utc>> {
    let mut candidate = now;

    // 最多搜索 2 年
    for _ in 0..(365 * 2 * 24 * 60) {
        candidate += TimeDelta::minutes(1);

        let c_min = candidate.minute() as i32;
        let c_hour = candidate.hour() as i32;
        let c_day = candidate.day() as i32;
        let c_month = candidate.month() as i32;
        let c_wday = candidate.weekday().num_days_from_sunday() as i32;

        let match_min = minute == -1 || c_min == minute;
        let match_hour = hour == -1 || c_hour == hour;
        let match_day = day == -1 || c_day == day;
        let match_month = month == -1 || c_month == month;
        let match_wday = weekday == -1 || c_wday == weekday;

        if match_min && match_hour && match_day && match_month && match_wday {
            return Some(candidate);
        }
    }

    None
}

/// 查找下一个匹配的时间点 (多值匹配).
fn find_next_match_multi(
    now: DateTime<Utc>,
    seconds: &[u8],
    minutes: &[u8],
    hours: &[u8],
    days_of_month: &[u8],
    months: &[u8],
    days_of_week: &[u8],
) -> Option<DateTime<Utc>> {
    let mut candidate = now;

    // 最多搜索 2 年
    for _ in 0..(365 * 2 * 24 * 3600) {
        candidate += TimeDelta::seconds(1);

        let c_sec = candidate.second() as u8;
        let c_min = candidate.minute() as u8;
        let c_hour = candidate.hour() as u8;
        let c_day = candidate.day() as u8;
        let c_month = candidate.month() as u8;
        let c_wday = candidate.weekday().num_days_from_sunday() as u8;

        let match_sec = seconds.contains(&c_sec);
        let match_min = minutes.contains(&c_min);
        let match_hour = hours.contains(&c_hour);
        let match_day = days_of_month.contains(&c_day) || days_of_month.is_empty();
        let match_month = months.contains(&c_month) || months.is_empty();
        let match_wday = days_of_week.contains(&c_wday) || days_of_week.is_empty();
        // 如果 dom 和 dow 都设置了，满足任一即可
        let day_ok = match_day || match_wday;

        if match_sec && match_min && match_hour && day_ok && match_month {
            return Some(candidate);
        }
    }

    None
}

// ═══════════════════════════════════════════════════════════
// CronManager — 管理定时任务的注册与触发
// ═══════════════════════════════════════════════════════════

/// Cron 作业条目.
struct CronEntry {
    name: String,
    schedule: CronSchedule,
    #[allow(dead_code)]
    job: Box<dyn Job>,
    enabled: bool,
    last_run: Option<DateTime<Utc>>,
    handle: Option<JoinHandle<()>>,
}

/// Cron 管理器 — 管理所有定时任务.
pub struct CronManager {
    entries: Arc<RwLock<HashMap<String, CronEntry>>>,
    scheduler: TaskScheduler,
    running: RwLock<bool>,
    main_handle: RwLock<Option<JoinHandle<()>>>,
}

impl CronManager {
    /// 创建 Cron 管理器.
    pub fn new(scheduler: TaskScheduler) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            scheduler,
            running: RwLock::new(false),
            main_handle: RwLock::new(None),
        }
    }

    /// 注册一个 cron 作业.
    pub async fn add_job(&self, name: &str, schedule: CronSchedule, job: Box<dyn Job>) -> LsResult<()> {
        let mut entries = self.entries.write().await;
        if entries.contains_key(name) {
            return Err(LsError::InvalidArgument(format!(
                "cron job '{name}' already exists"
            )));
        }

        let entry = CronEntry {
            name: name.to_string(),
            schedule,
            job,
            enabled: true,
            last_run: None,
            handle: None,
        };
        entries.insert(name.to_string(), entry);

        info!(name = %name, "cron job registered");
        Ok(())
    }

    /// 移除一个 cron 作业.
    pub async fn remove_job(&self, name: &str) -> LsResult<()> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.remove(name) {
            if let Some(handle) = entry.handle {
                handle.abort();
            }
            info!(name = %name, "cron job removed");
            Ok(())
        } else {
            Err(LsError::NotFound(format!("cron job '{name}'")))
        }
    }

    /// 启动 Cron 调度循环.
    pub async fn start(&self) -> LsResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(LsError::RuntimeState("cron manager already running".into()));
        }
        *running = true;

        let entries = self.entries.clone();
        let scheduler = self.scheduler.clone();

        let handle = tokio::spawn(async move {
            Self::run_loop(entries, scheduler).await;
        });

        *self.main_handle.write().await = Some(handle);
        info!("cron manager started");
        Ok(())
    }

    /// 停止 Cron 调度循环.
    pub async fn shutdown(&self) {
        let mut running = self.running.write().await;
        *running = false;

        if let Some(handle) = self.main_handle.write().await.take() {
            handle.abort();
        }

        // 停止所有作业
        let mut entries = self.entries.write().await;
        for (_, entry) in entries.iter_mut() {
            if let Some(handle) = entry.handle.take() {
                handle.abort();
            }
        }

        info!("cron manager shut down");
    }

    /// 列出所有 cron 作业.
    pub async fn list_jobs(&self) -> Vec<CronJobSummary> {
        let entries = self.entries.read().await;
        entries
            .values()
            .map(|e| CronJobSummary {
                name: e.name.clone(),
                expression: e.schedule.expression().to_string(),
                enabled: e.enabled,
                last_run: e.last_run.map(|t| t.to_rfc3339()),
            })
            .collect()
    }

    async fn run_loop(entries: Arc<RwLock<HashMap<String, CronEntry>>>, _scheduler: TaskScheduler) {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;

            let now = Utc::now();
            let job_names: Vec<String> = {
                let entries_guard = entries.read().await;
                entries_guard
                    .iter()
                    .filter(|(_, e)| e.enabled)
                    .filter(|(_, e)| {
                        let next = e.schedule.next_occurrence(e.last_run);
                        next.is_some_and(|n| n <= now)
                    })
                    .map(|(name, _)| name.clone())
                    .collect()
            };

            for name in job_names {
                let mut entries_guard = entries.write().await;
                if let Some(entry) = entries_guard.get_mut(&name) {
                    entry.last_run = Some(now);

                    // 克隆 job 并通过调度器提交
                    // 注意: 这里简化处理，实际应使用 Job 的 clone 或工厂模式
                    // 对于 cron 作业，我们创建一次性执行
                    info!(name = %name, "cron job triggered");
                }
            }
        }
    }
}

/// Cron 作业摘要.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobSummary {
    pub name: String,
    pub expression: String,
    pub enabled: bool,
    pub last_run: Option<String>,
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_standard() {
        let sched = CronSchedule::parse("*/5 * * * *").unwrap();
        let next = sched.next_occurrence(None);
        assert!(next.is_some());
    }

    #[test]
    fn test_parse_with_seconds() {
        let sched = CronSchedule::parse("0 */5 * * * *").unwrap();
        let next = sched.next_occurrence(None);
        assert!(next.is_some());
    }

    #[test]
    fn test_parse_every() {
        let sched = CronSchedule::parse("@every 30s").unwrap();
        let next = sched.next_occurrence(None);
        assert!(next.is_some());
    }

    #[test]
    fn test_parse_presets() {
        for expr in &["@hourly", "@daily", "@weekly", "@monthly", "@yearly"] {
            let sched = CronSchedule::parse(expr).unwrap();
            assert!(sched.next_occurrence(None).is_some());
        }
    }

    #[test]
    fn test_parse_invalid_too_few_fields() {
        assert!(CronSchedule::parse("* * * *").is_err());
    }

    #[test]
    fn test_parse_field_star() {
        let result = parse_field("*", 0, 59).unwrap();
        assert_eq!(result.len(), 60);
    }

    #[test]
    fn test_parse_field_step() {
        let result = parse_field("*/10", 0, 59).unwrap();
        assert_eq!(result, vec![0, 10, 20, 30, 40, 50]);
    }

    #[test]
    fn test_parse_field_range() {
        let result = parse_field("1-5", 0, 59).unwrap();
        assert_eq!(result, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_parse_field_list() {
        let result = parse_field("1,3,5", 0, 59).unwrap();
        assert_eq!(result, vec![1, 3, 5]);
    }

    #[test]
    fn test_parse_field_out_of_range() {
        assert!(parse_field("60", 0, 59).is_err());
    }

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn test_parse_duration_combined() {
        assert_eq!(parse_duration("1h30m").unwrap(), Duration::from_secs(5400));
    }

    #[test]
    fn test_parse_duration_days() {
        assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(86400));
    }

    #[test]
    fn test_next_occurrence_every() {
        let sched = CronSchedule::parse("@every 1h").unwrap();
        let now = Utc::now();
        let next = sched.next_occurrence(Some(now)).unwrap();
        let diff = next - now;
        assert!(diff.num_seconds() >= 3550 && diff.num_seconds() <= 3650);
    }
}
