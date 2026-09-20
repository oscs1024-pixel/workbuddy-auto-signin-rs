use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use serde_json::{json, Value};

use super::json::serialize_safe;

#[derive(Debug, Clone)]
pub struct Reporter {
    action: String,
    config_warning: Option<String>,
    default_log: PathBuf,
}

impl Reporter {
    pub fn new(action: impl Into<String>, config_warning: Option<String>) -> Self {
        let default_log = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("signin.log");

        Self {
            action: action.into(),
            config_warning,
            default_log,
        }
    }

    pub fn emit(&self, mut out: Value) {
        if let (Some(warning), Some(object)) = (self.config_warning.as_ref(), out.as_object_mut()) {
            object.insert("config_warning".into(), json!(warning));
        }

        let payload = serialize_safe(&out);
        let is_error = out.get("result").and_then(Value::as_str) == Some("ERROR");

        // 普通交互命令默认输出易读摘要；silent 与调试命令继续使用稳定 JSON。
        if !self.action.starts_with("silent") {
            let rendered = if self.use_json_stdout() {
                payload.clone()
            } else {
                render_human(&self.action, &out)
            };
            let stdout_ok = writeln!(io::stdout().lock(), "{rendered}").is_ok();
            if stdout_ok && !is_error {
                return;
            }
        }

        // 文件日志始终保持单行 JSON，便于脚本检索和后续自动处理。
        let line = format!("[{}] {payload}\n", Local::now().format("%Y-%m-%d %H:%M:%S"));

        let requested = std::env::var_os("WORKBUDDY_SIGNIN_LOG")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_log.clone());

        // 自定义日志路径不可写时回退到默认路径，尽量保证计划任务的失败信息不会无声丢失。
        for path in unique_paths(requested, self.default_log.clone()) {
            if append(&path, line.as_bytes()).is_ok() {
                return;
            }
        }
    }

    fn use_json_stdout(&self) -> bool {
        if matches!(self.action.as_str(), "status" | "claim" | "all") {
            return true;
        }

        std::env::var("WORKBUDDY_OUTPUT")
            .ok()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("json"))
    }

}

fn render_human(action: &str, out: &Value) -> String {
    // growth 命令即使提前返回 NETWORK/NO_SESSION/TIMEOUT，也必须保持“成长中心”语境。
    if action == "growth" || out.get("result").and_then(Value::as_str) == Some("GROWTH") {
        let mut lines = vec!["成长中心".to_string()];
        render_growth(&mut lines, out);
        if let Some(warning) = out.get("config_warning").and_then(Value::as_str) {
            lines.push(String::new());
            lines.push(format!("提示：{warning}"));
        }
        return lines.join("\n");
    }

    let mut lines = vec!["签到".to_string()];
    render_signin(&mut lines, out);

    if let Some(detail) = out.get("growth_detail").filter(|value| value.is_object()) {
        lines.push(String::new());
        lines.push("成长中心".to_string());
        render_growth(&mut lines, detail);
    } else if let Some(growth) = out.get("growth").and_then(Value::as_str) {
        lines.push(String::new());
        lines.push("成长中心".to_string());
        lines.push(format!("  • {growth}"));
    }

    if let Some(warning) = out.get("config_warning").and_then(Value::as_str) {
        lines.push(String::new());
        lines.push(format!("提示：{warning}"));
    }

    lines.join("\n")
}

fn render_signin(lines: &mut Vec<String>, out: &Value) {
    let result = out.get("result").and_then(Value::as_str).unwrap_or("");
    let status = match result {
        "CLAIMED" => "✓ 签到成功",
        "ALREADY" => "✓ 今日已完成",
        "INACTIVE" => "• 活动未开启",
        "NO_SESSION" => "! 登录态失效",
        "NO_AUTH" => "! 未找到登录凭据",
        "NETWORK" => "! 网络不可达",
        "TIMEOUT" => "! 时间预算耗尽",
        "ERROR" => "! 执行失败",
        "UNKNOWN" => "! 返回结果无法识别",
        _ => "",
    };

    if !status.is_empty() {
        lines.push(format!("  {status}"));
    }

    push_metric(lines, "今日积分", out.get("today_credit"), true);
    push_metric(lines, "本次积分", out.get("credit"), true);
    push_metric(lines, "连续签到", out.get("streak_days"), false);
    push_metric(lines, "累计积分", out.get("total_credits"), false);

    if matches!(
        result,
        "NO_SESSION" | "NO_AUTH" | "NETWORK" | "TIMEOUT" | "ERROR" | "UNKNOWN" | "INACTIVE"
    ) {
        if let Some(report) = out.get("report").and_then(Value::as_str) {
            lines.push(format!("  {report}"));
        }
    }
}

fn push_metric(lines: &mut Vec<String>, label: &str, value: Option<&Value>, signed: bool) {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return;
    };

    let text = display_scalar(value);
    let text = match label {
        "连续签到" => format!("{text} 天"),
        "累计积分" => format!("{text} 积分"),
        _ if signed && !text.starts_with('-') && !text.starts_with('+') => format!("+{text}"),
        _ => text,
    };
    lines.push(format!("  {label:<8} {text}"));
}

fn render_growth(lines: &mut Vec<String>, detail: &Value) {
    let result = detail.get("result").and_then(Value::as_str).unwrap_or("GROWTH");
    if result != "GROWTH" {
        let report = detail
            .get("report")
            .and_then(Value::as_str)
            .unwrap_or("成长中心执行失败");
        lines.push(format!("  ! {report}"));
        return;
    }

    let items = detail
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut travel = Vec::new();
    let mut accepted_tasks = Vec::new();
    let mut task_other = Vec::new();
    let mut makeup = Vec::new();
    let mut rewards = Vec::new();
    let mut other = Vec::new();

    for item in items.iter().filter_map(Value::as_str) {
        if let Some(title) = item
            .strip_prefix("已接取任务「")
            .and_then(|value| value.strip_suffix('」'))
        {
            accepted_tasks.push(title.to_string());
        } else if item.contains("任务") {
            task_other.push(item.to_string());
        } else if item.starts_with("补登") || item.contains("无需补登") || item.starts_with("另有 ")
        {
            makeup.push(item.to_string());
        } else if item.contains("旅行")
            || item.starts_with("Buddy 已前往")
            || item.starts_with("领旅行礼物")
        {
            travel.push(item.to_string());
        } else if item.starts_with("连登兑换")
            || item.starts_with("开盲盒")
            || item.starts_with("还剩 ")
            || item.starts_with("开 Buddy 盲盒")
        {
            rewards.push(item.to_string());
        } else {
            other.push(item.to_string());
        }
    }

    push_group(lines, "旅行", &travel);

    if !accepted_tasks.is_empty() || !task_other.is_empty() {
        if accepted_tasks.is_empty() {
            lines.push("  任务".to_string());
        } else {
            lines.push(format!("  任务（已接取 {} 个）", accepted_tasks.len()));
            for title in accepted_tasks {
                lines.push(format!("    • {title}"));
            }
        }
        for item in task_other {
            lines.push(format!("    • {item}"));
        }
    }

    push_group(lines, "补登", &makeup);
    push_group(lines, "奖励", &rewards);
    push_group(lines, "其他", &other);

    let mut status = Vec::new();
    if let Some(value) = detail.get("energy").filter(|value| !value.is_null()) {
        status.push(format!("能量 {}", display_scalar(value)));
    }
    if let Some(value) = detail.get("streak_days").filter(|value| !value.is_null()) {
        status.push(format!("连签 {} 天", display_scalar(value)));
    }
    if let Some(value) = detail
        .get("credits_gained")
        .filter(|value| value.as_i64().unwrap_or(0) != 0)
    {
        status.push(format!("本次 +{} 积分", display_scalar(value)));
    }

    if !status.is_empty() {
        lines.push("  状态".to_string());
        lines.push(format!("    {}", status.join(" · ")));
    }

    if items.is_empty() && status.is_empty() {
        if let Some(report) = detail.get("report").and_then(Value::as_str) {
            lines.push(format!("  • {report}"));
        } else {
            lines.push("  • 无可处理项目".to_string());
        }
    }
}

fn push_group(lines: &mut Vec<String>, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }

    lines.push(format!("  {title}"));
    for item in items {
        lines.push(format!("    • {item}"));
    }
}

fn display_scalar(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn unique_paths(first: PathBuf, second: PathBuf) -> Vec<PathBuf> {
    if first == second {
        vec![first]
    } else {
        vec![first, second]
    }
}

fn append(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::render_human;

    #[test]
    fn daily_human_output_is_grouped_and_readable() {
        let out = json!({
            "result": "ALREADY",
            "today_credit": 100,
            "streak_days": 5,
            "total_credits": 500,
            "growth_detail": {
                "items": [
                    "Buddy 已前往咖啡馆（即将返回）",
                    "已接取任务「体验「公益专家」」",
                    "已接取任务「和 AI 聊天 5 次」",
                    "2026-09-06 无需补登",
                    "另有 1 天可补、剩 1 张卡，下轮继续"
                ],
                "energy": 0,
                "streak_days": 20,
                "credits_gained": 0
            }
        });

        let rendered = render_human("auto", &out);
        assert!(rendered.contains("✓ 今日已完成"));
        assert!(rendered.contains("任务（已接取 2 个）"));
        assert!(rendered.contains("2026-09-06 无需补登"));
        assert!(rendered.contains("能量 0 · 连签 20 天"));
        assert!(!rendered.starts_with('{'));
    }

    #[test]
    fn growth_action_keeps_growth_heading_on_network_error() {
        let out = json!({
            "result": "NETWORK",
            "report": "网络不可达，成长中心跳过"
        });

        let rendered = render_human("growth", &out);
        assert!(rendered.starts_with("成长中心\n"));
        assert!(rendered.contains("网络不可达，成长中心跳过"));
        assert!(!rendered.starts_with("签到"));
    }

    #[test]
    fn daily_growth_failure_is_not_rendered_as_empty() {
        let out = json!({
            "result": "ALREADY",
            "today_credit": 100,
            "growth_detail": {
                "result": "NO_SESSION",
                "report": "登录态已失效，请重新登录 WorkBuddy 桌面端",
                "items": [],
                "energy": null,
                "streak_days": null,
                "credits_gained": 0,
                "idle": false
            }
        });

        let rendered = render_human("auto", &out);
        assert!(rendered.contains("成长中心"));
        assert!(rendered.contains("登录态已失效"));
        assert!(!rendered.contains("无可处理项目"));
    }
}
