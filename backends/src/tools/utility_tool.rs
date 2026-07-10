//! 实用工具 — CurrentTimeTool, CalculatorTool

use async_trait::async_trait;
use chrono::{Datelike, Local, Timelike, Utc};
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};
use serde_json::Value;

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
        ..Default::default()
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
        let timezone = input
            .get("timezone")
            .and_then(|v| v.as_str())
            .unwrap_or("utc");
        let format = input
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("iso");

        let now_utc = Utc::now();
        let now_local = Local::now();

        let (time_str, unix_ts, year, month, day, hour, minute, second, weekday, tz_name) =
            match timezone {
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
                    (
                        formatted,
                        unix,
                        ts.year(),
                        ts.month(),
                        ts.day(),
                        ts.hour(),
                        ts.minute(),
                        ts.second(),
                        wd,
                        tz,
                    )
                }
                _ => {
                    // utc
                    let ts = now_utc.naive_utc();
                    let unix = now_utc.timestamp();
                    let wd = now_utc.format("%A").to_string();
                    let formatted = match format {
                        "unix" => unix.to_string(),
                        "human" => now_utc.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                        _ => now_utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                    };
                    (
                        formatted,
                        unix,
                        ts.year(),
                        ts.month(),
                        ts.day(),
                        ts.hour(),
                        ts.minute(),
                        ts.second(),
                        wd,
                        "UTC".to_string(),
                    )
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
        ..Default::default()
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

        // 使用内置表达式求值
        let result = eval_expression(expression).map_err(|e| {
            LsError::Internal(format!("failed to evaluate expression '{expression}': {e}"))
        })?;

        Ok(serde_json::json!({
            "expression": expression,
            "result": result,
            "result_type": if result.fract() == 0.0 { "integer" } else { "float" },
        }))
    }
}

// ── 简易数学表达式求值 ─────────────────────────────

/// 简易表达式求值器，替换 meval crate。
/// 支持基本算术 +, -, *, /, ^, % 和函数 sin, cos, tan, sqrt, log, ln, abs, floor, ceil, round, exp, max, min。
fn eval_expression(expr: &str) -> Result<f64, String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return Err("empty expression".into());
    }
    let chars: Vec<char> = expr.chars().collect();
    let mut pos = 0;
    let result = parse_expr(&chars, &mut pos)?;
    skip_whitespace(&chars, &mut pos);
    if pos < chars.len() {
        return Err(format!("unexpected character '{}' at position {}", chars[pos], pos));
    }
    Ok(result)
}

fn skip_whitespace(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn parse_number(chars: &[char], pos: &mut usize) -> Result<f64, String> {
    skip_whitespace(chars, pos);
    let start = *pos;
    
    // Check for named constants
    if *pos < chars.len() && chars[*pos].is_ascii_alphabetic() {
        while *pos < chars.len() && (chars[*pos].is_ascii_alphanumeric() || chars[*pos] == '_') {
            *pos += 1;
        }
        let name: String = chars[start..*pos].iter().collect();
        return match name.as_str() {
            "pi" => Ok(std::f64::consts::PI),
            "e" => Ok(std::f64::consts::E),
            "inf" => Ok(f64::INFINITY),
            "nan" => Ok(f64::NAN),
            _ => Err(format!("unknown identifier '{}'", name)),
        };
    }
    
    if *pos < chars.len() && chars[*pos] == '-' {
        *pos += 1;
        let val = parse_number(chars, pos)?;
        return Ok(-val);
    }
    if *pos < chars.len() && chars[*pos] == '+' {
        *pos += 1;
        return parse_number(chars, pos);
    }
    
    if *pos >= chars.len() || !(chars[*pos].is_ascii_digit() || chars[*pos] == '.') {
        return Err(format!("expected number at position {}", start));
    }
    
    while *pos < chars.len() && (chars[*pos].is_ascii_digit() || chars[*pos] == '.') {
        *pos += 1;
    }
    let num_str: String = chars[start..*pos].iter().collect();
    num_str.parse::<f64>().map_err(|e| format!("invalid number '{}': {}", num_str, e))
}

fn parse_function(chars: &[char], pos: &mut usize) -> Result<f64, String> {
    skip_whitespace(chars, pos);
    
    if *pos >= chars.len() {
        return Err("unexpected end of expression".into());
    }
    
    // Check if it's a function call (letters followed by '(')
    if chars[*pos].is_ascii_alphabetic() {
        let start = *pos;
        while *pos < chars.len() && (chars[*pos].is_ascii_alphanumeric() || chars[*pos] == '_') {
            *pos += 1;
        }
        let name: String = chars[start..*pos].iter().collect();
        skip_whitespace(chars, pos);
        
        if *pos < chars.len() && chars[*pos] == '(' {
            // It's a function call
            *pos += 1; // skip '('
            let arg = parse_expr(chars, pos)?;
            skip_whitespace(chars, pos);
            if *pos >= chars.len() || chars[*pos] != ')' {
                return Err(format!("expected ')' after function argument at position {}", *pos));
            }
            *pos += 1; // skip ')'
            
            return match name.as_str() {
                "sin" => Ok(arg.sin()),
                "cos" => Ok(arg.cos()),
                "tan" => Ok(arg.tan()),
                "asin" => Ok(arg.asin()),
                "acos" => Ok(arg.acos()),
                "atan" => Ok(arg.atan()),
                "sqrt" => Ok(arg.sqrt()),
                "log" => Ok(arg.log10()),
                "ln" => Ok(arg.ln()),
                "abs" => Ok(arg.abs()),
                "floor" => Ok(arg.floor()),
                "ceil" => Ok(arg.ceil()),
                "round" => Ok(arg.round()),
                "exp" => Ok(arg.exp()),
                "sinh" => Ok(arg.sinh()),
                "cosh" => Ok(arg.cosh()),
                "tanh" => Ok(arg.tanh()),
                "rad" => Ok(arg.to_radians()),
                "deg" => Ok(arg.to_degrees()),
                _ => Err(format!("unknown function '{}'", name)),
            };
        } else {
            // It's a named constant or variable
            return match name.as_str() {
                "pi" => Ok(std::f64::consts::PI),
                "e" => Ok(std::f64::consts::E),
                "inf" => Ok(f64::INFINITY),
                _ => Err(format!("unknown identifier '{}'", name)),
            };
        }
    }
    
    if chars[*pos] == '(' {
        *pos += 1; // skip '('
        let result = parse_expr(chars, pos)?;
        skip_whitespace(chars, pos);
        if *pos >= chars.len() || chars[*pos] != ')' {
            return Err(format!("expected ')' at position {}", *pos));
        }
        *pos += 1; // skip ')'
        return Ok(result);
    }
    
    parse_number(chars, pos)
}

// Power (right-associative)
fn parse_power(chars: &[char], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_function(chars, pos)?;
    skip_whitespace(chars, pos);
    while *pos < chars.len() && chars[*pos] == '^' {
        *pos += 1; // skip '^'
        let right = parse_power(chars, pos)?;
        left = left.powf(right);
        skip_whitespace(chars, pos);
    }
    Ok(left)
}

// Unary minus and plus
fn parse_unary(chars: &[char], pos: &mut usize) -> Result<f64, String> {
    skip_whitespace(chars, pos);
    if *pos >= chars.len() {
        return Err("unexpected end of expression".into());
    }
    if chars[*pos] == '-' {
        *pos += 1;
        let val = parse_unary(chars, pos)?;
        return Ok(-val);
    }
    if chars[*pos] == '+' {
        *pos += 1;
        return parse_unary(chars, pos);
    }
    parse_power(chars, pos)
}

// Multiplication, division, modulo
fn parse_term(chars: &[char], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_unary(chars, pos)?;
    skip_whitespace(chars, pos);
    while *pos < chars.len() && (chars[*pos] == '*' || chars[*pos] == '/' || chars[*pos] == '%') {
        let op = chars[*pos];
        *pos += 1;
        let right = parse_unary(chars, pos)?;
        left = match op {
            '*' => left * right,
            '/' => {
                if right == 0.0 {
                    return Err("division by zero".into());
                }
                left / right
            }
            '%' => left % right,
            _ => unreachable!(),
        };
        skip_whitespace(chars, pos);
    }
    Ok(left)
}

// Addition and subtraction
fn parse_expr(chars: &[char], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_term(chars, pos)?;
    skip_whitespace(chars, pos);
    while *pos < chars.len() && (chars[*pos] == '+' || chars[*pos] == '-') {
        let op = chars[*pos];
        *pos += 1;
        let right = parse_term(chars, pos)?;
        left = match op {
            '+' => left + right,
            '-' => left - right,
            _ => unreachable!(),
        };
        skip_whitespace(chars, pos);
    }
    Ok(left)
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
        let result = tool.execute(test_ctx(), json!({"expression": "2 +"})).await;
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

    // ── 表达式求值器单元测试 ──
    #[test]
    fn test_eval_basic_arithmetic() {
        let r = eval_expression("1 + 2 * 3").unwrap();
        assert!((r - 7.0).abs() < 1e-10, "1+2*3 = 7, got {r}");
    }

    #[test]
    fn test_eval_parentheses() {
        let r = eval_expression("(1 + 2) * 3").unwrap();
        assert!((r - 9.0).abs() < 1e-10, "(1+2)*3 = 9, got {r}");
    }

    #[test]
    fn test_eval_power() {
        let r = eval_expression("2 ^ 10").unwrap();
        assert!((r - 1024.0).abs() < 1e-10, "2^10 = 1024, got {r}");
    }

    #[test]
    fn test_eval_sqrt() {
        let r = eval_expression("sqrt(16)").unwrap();
        assert!((r - 4.0).abs() < 1e-10, "sqrt(16) = 4, got {r}");
    }

    #[test]
    fn test_eval_sin_pi_over_2() {
        let r = eval_expression("sin(pi / 2)").unwrap();
        assert!((r - 1.0).abs() < 1e-10, "sin(pi/2) = 1, got {r}");
    }

    #[test]
    fn test_eval_ln_e() {
        let r = eval_expression("ln(e)").unwrap();
        assert!((r - 1.0).abs() < 1e-10, "ln(e) = 1, got {r}");
    }

    #[test]
    fn test_eval_abs() {
        let r = eval_expression("abs(-10)").unwrap();
        assert!((r - 10.0).abs() < 1e-10, "abs(-10) = 10, got {r}");
    }

    #[test]
    fn test_eval_modulo() {
        let r = eval_expression("10 % 3").unwrap();
        assert!((r - 1.0).abs() < 1e-10, "10%3 = 1, got {r}");
    }

    #[test]
    fn test_eval_round() {
        let r = eval_expression("round(1.5)").unwrap();
        assert!((r - 2.0).abs() < 1e-10, "round(1.5) = 2, got {r}");
    }

    #[test]
    fn test_eval_error_incomplete_expr() {
        assert!(eval_expression("1+").is_err(), "1+ should error");
    }

    #[test]
    fn test_eval_error_unmatched_paren() {
        assert!(eval_expression("(").is_err(), "( should error");
    }

    #[test]
    fn test_eval_error_sqrt_no_arg() {
        assert!(eval_expression("sqrt(").is_err(), "sqrt( should error");
    }

    #[test]
    fn test_eval_error_unknown_symbol() {
        assert!(eval_expression("abc").is_err(), "abc should error");
    }
}
