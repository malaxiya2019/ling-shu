//! 实用工具 — CurrentTimeTool, CalculatorTool

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};
use serde_json::Value;
use chrono::{Utc, Local, Datelike, Timelike};

/// 当前时间工具.
pub struct CurrentTimeTool;

#[async_trait]
impl Tool for CurrentTimeTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "current_time".into(),
            description: "获取当前日期和时间信息。支持 UTC 和本地时区。".into(),
            parameters: vec![
                ToolParam {
                    name: "timezone".into(),
                    description: "时区: 'utc' (默认) 或 'local'".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "format".into(),
                    description: "输出格式: 'iso' (默认), 'unix', 'human' (可读格式)".into(),
                    required: false,
                    param_type: "string".into(),
                },
            ],
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if let Some(tz) = input.get("timezone").and_then(|v| v.as_str()) {
            if tz != "utc" && tz != "local" {
                return Err(LsError::Validation(format!(
                    "invalid timezone '{tz}': must be 'utc' or 'local'"
                )));
            }
        }
        if let Some(fmt) = input.get("format").and_then(|v| v.as_str()) {
            if fmt != "iso" && fmt != "unix" && fmt != "human" {
                return Err(LsError::Validation(format!(
                    "invalid format '{fmt}': must be 'iso', 'unix', or 'human'"
                )));
            }
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        self.validate(&input)?;
        let timezone = input.get("timezone").and_then(|v| v.as_str()).unwrap_or("utc");
        let format = input.get("format").and_then(|v| v.as_str()).unwrap_or("iso");

        let now_utc = Utc::now();
        let now_local = Local::now();

        let (time_str, unix_ts, year, month, day, hour, minute, second, weekday, tz_name) = match timezone {
            "local" => {
                let ts = now_local.naive_local();
                let unix = now_local.timestamp();
                let wd = now_local.format("%A").to_string();
                let tz = now_local.format("%Z").to_string();
                let formatted = match format {
                    "unix" => unix.to_string(),
                    "human" => now_local.format("%Y-%m-%d %H:%M:%S %Z").to_string(),
                    _ => now_local.format("%Y-%m-%dT%H:%M:%S%z").to_string(),
                };
                (formatted, unix, ts.year(), ts.month(), ts.day(), ts.hour(), ts.minute(), ts.second(), wd, tz)
            }
            _ => { // utc
                let ts = now_utc.naive_utc();
                let unix = now_utc.timestamp();
                let wd = now_utc.format("%A").to_string();
                let formatted = match format {
                    "unix" => unix.to_string(),
                    "human" => now_utc.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                    _ => now_utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                };
                (formatted, unix, ts.year(), ts.month(), ts.day(), ts.hour(), ts.minute(), ts.second(), wd, "UTC".to_string())
            }
        };

        Ok(serde_json::json!({
            "timezone": tz_name,
            "timestamp": time_str,
            "unix_timestamp": unix_ts,
            "datetime": {
                "year": year,
                "month": month,
                "day": day,
                "hour": hour,
                "minute": minute,
                "second": second,
                "weekday": weekday,
            },
            "format": format,
        }))
    }
}

/// 计算器工具 (安全的数学表达式求值).
pub struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "calculator".into(),
            description: "执行数学计算。支持: +, -, *, /, %, 以及数学函数: sqrt, abs, sin, cos, tan, ln, log10, exp, ceil, floor, round, min, max。".into(),
            parameters: vec![
                ToolParam {
                    name: "expression".into(),
                    description: "数学表达式，如 '2 + 3 * 4', 'sqrt(144)', 'max(10, 20)'".into(),
                    required: true,
                    param_type: "string".into(),
                },
            ],
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        let expr = input
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LsError::Validation("missing required field: expression".into()))?;

        if expr.trim().is_empty() {
            return Err(LsError::Validation("expression must not be empty".into()));
        }

        // 基本安全检查: 只允许数学字符
        let allowed_chars = |c: char| {
            c.is_ascii_digit()
                || c.is_ascii_whitespace()
                || "+-*/%().,^_".contains(c)
                || c.is_ascii_alphabetic()
        };
        if !expr.chars().all(allowed_chars) {
            return Err(LsError::Validation(
                "expression contains disallowed characters".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        self.validate(&input)?;
        let expression = input["expression"].as_str().unwrap();

        // 使用 meval crate 进行安全求值
        let result = meval::eval_str(expression)
            .map_err(|e| LsError::Internal(format!("failed to evaluate expression '{expression}': {e}")))?;

        Ok(serde_json::json!({
            "expression": expression,
            "result": result,
            "result_type": if result.fract() == 0.0 { "integer" } else { "float" },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use serde_json::json;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_current_time_utc() {
        let tool = CurrentTimeTool;
        let result = tool
            .execute(test_ctx(), json!({"timezone": "utc", "format": "iso"}))
            .await
            .unwrap();
        assert_eq!(result["timezone"], "UTC");
        assert!(result["unix_timestamp"].as_i64().unwrap() > 1_700_000_000);
        assert!(result["timestamp"].as_str().unwrap().contains("T"));
    }

    #[tokio::test]
    async fn test_current_time_unix() {
        let tool = CurrentTimeTool;
        let result = tool
            .execute(test_ctx(), json!({"format": "unix"}))
            .await
            .unwrap();
        // Unix timestamp should be a number string
        let ts: i64 = result["timestamp"].as_str().unwrap().parse().unwrap();
        assert!(ts > 1_700_000_000);
    }

    #[tokio::test]
    async fn test_current_time_human() {
        let tool = CurrentTimeTool;
        let result = tool
            .execute(test_ctx(), json!({"format": "human"}))
            .await
            .unwrap();
        let ts = result["timestamp"].as_str().unwrap();
        assert!(ts.contains("UTC"));
    }

    #[tokio::test]
    async fn test_current_time_invalid_tz() {
        let tool = CurrentTimeTool;
        let result = tool.validate(&json!({"timezone": "invalid"}));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_calculator_simple() {
        let tool = CalculatorTool;
        let result = tool
            .execute(test_ctx(), json!({"expression": "2 + 3 * 4"}))
            .await
            .unwrap();
        assert!((result["result"].as_f64().unwrap() - 14.0).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_calculator_sqrt() {
        let tool = CalculatorTool;
        let result = tool
            .execute(test_ctx(), json!({"expression": "sqrt(144)"}))
            .await
            .unwrap();
        assert!((result["result"].as_f64().unwrap() - 12.0).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_calculator_pow() {
        let tool = CalculatorTool;
        let result = tool
            .execute(test_ctx(), json!({"expression": "2*2*2*2*2*2*2*2*2*2"}))
            .await
            .unwrap();
        assert!((result["result"].as_f64().unwrap() - 1024.0).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_calculator_invalid_expr() {
        let tool = CalculatorTool;
        let result = tool
            .execute(test_ctx(), json!({"expression": "2 +"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_calculator_validation() {
        let tool = CalculatorTool;
        // Disallowed chars
        let result = tool
            .execute(test_ctx(), json!({"expression": "2 + 3; rm -rf /"}))
            .await;
        assert!(result.is_err());
    }
}
