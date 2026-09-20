use serde_json::{json, Map, Value};

use crate::api::BillingApi;
use crate::http::{HttpResult, CODE_BUDGET_OUT, CODE_NO_NETWORK};
use crate::model::common::ServiceRun;
use crate::util::{dig, format_credit, is_true_or_one, value_truthy};

#[derive(Clone)]
pub struct SigninService {
    api: BillingApi,
}

impl SigninService {
    pub fn new(api: BillingApi) -> Self {
        Self { api }
    }

    pub async fn raw_status(&self) -> HttpResult {
        self.api.status().await
    }

    pub async fn raw_claim(&self) -> HttpResult {
        self.api.claim().await
    }

    pub async fn run(&self) -> ServiceRun {
        // 先查状态再领取，保证重复执行时不会主动制造重复写请求。
        let status_result = self.api.status().await;

        if status_result.code == CODE_BUDGET_OUT {
            return ServiceRun {
                code: 1,
                out: json!({
                    "result": "TIMEOUT",
                    "report": "已达本次运行时间预算，签到跳过，下次自动重试"
                }),
            };
        }

        if status_result.code == CODE_NO_NETWORK {
            let error = dig(&status_result.body, "error")
                .and_then(Value::as_str)
                .unwrap_or("");
            return ServiceRun {
                code: 1,
                out: json!({
                    "result": "NETWORK",
                    "report": format!("网络不可达，签到跳过，下次自动重试（{error}）"),
                    "error": error
                }),
            };
        }

        if matches!(status_result.code, 401 | 403) {
            return no_session_with_http(status_result.code);
        }

        if !status_result.is_success() {
            return ServiceRun {
                code: 1,
                out: json!({
                    "result": "ERROR",
                    "report": format!(
                        "签到接口返回异常（HTTP {}），请重新登录客户端或稍后重试",
                        status_result.code
                    ),
                    "http": status_result.code,
                    "status_body": status_result.body
                }),
            };
        }

        let status = status_result.body;

        if matches!(dig(&status, "active"), Some(Value::Bool(false))) {
            let activity_name = dig(&status, "activity_name").and_then(Value::as_str);
            let report = activity_name
                .map(|name| format!("签到活动未开启（{name}）"))
                .unwrap_or_else(|| "签到活动未开启".to_string());
            return ServiceRun {
                code: 0,
                out: json!({
                    "result": "INACTIVE",
                    "report": report,
                    "active": false
                }),
            };
        }

        if is_true_or_one(dig(&status, "today_checked_in")) {
            return ServiceRun {
                code: 0,
                out: already_report(&status, None),
            };
        }

        // daily-checkin 服务端按“当天”幂等，因此它是少数允许网络/5xx 重试的写接口。
        let claim = self.api.claim().await;

        if matches!(claim.code, CODE_NO_NETWORK | CODE_BUDGET_OUT) {
            let error = dig(&claim.body, "error")
                .and_then(Value::as_str)
                .unwrap_or("");
            let result = if claim.code == CODE_NO_NETWORK {
                "NETWORK"
            } else {
                "TIMEOUT"
            };
            return ServiceRun {
                code: 1,
                out: json!({
                    "result": result,
                    "report": format!("领取请求未能送达，下次自动重试（{error}）")
                }),
            };
        }

        if matches!(claim.code, 401 | 403) {
            return no_session_with_http(claim.code);
        }

        // 兼容服务端两种“已签到”形态：JSON null，或业务码/文案提示已签。
        if is_already_checked_in(&claim.body) {
            let fresh_result = self.api.status().await;
            let fresh = if fresh_result.is_success() && fresh_result.body.is_object() {
                fresh_result.body
            } else {
                status.clone()
            };

            return ServiceRun {
                code: 0,
                out: already_report(&fresh, Some("今日已签过（服务端判定已领取）")),
            };
        }

        if let Some(credit) = dig(&claim.body, "credit").cloned() {
            // 领取成功后再查一次状态，尽量返回最新连签天数和累计积分；刷新失败则回退到领取前状态。
            let fresh_result = self.api.status().await;
            let fresh = if fresh_result.is_success() && fresh_result.body.is_object() {
                fresh_result.body
            } else {
                status.clone()
            };

            let streak_days = dig(&fresh, "streak_days")
                .cloned()
                .or_else(|| dig(&status, "streak_days").cloned());
            let total_credits = dig(&fresh, "total_credits").cloned();
            let is_streak_day = dig(&fresh, "is_streak_day").cloned();
            let next_streak_day = dig(&fresh, "next_streak_day").cloned();

            let bonus = if value_truthy(is_streak_day.as_ref()) {
                "，且为连签奖励日"
            } else {
                ""
            };

            let cumulative = total_credits
                .as_ref()
                .map(|value| format!("，累计 {} 积分", format_credit(value)))
                .unwrap_or_default();

            let streak = match streak_days.as_ref() {
                Some(days) => format!(
                    "（连续 {} 天{}）",
                    display_value(days),
                    cumulative
                ),
                None if !cumulative.is_empty() => {
                    format!("（{}）", cumulative.trim_start_matches('，'))
                }
                None => String::new(),
            };

            let report = format!(
                "成功领取 {} 积分{}{}",
                format_credit(&credit),
                bonus,
                streak
            );

            let mut out = Map::new();
            out.insert("result".into(), json!("CLAIMED"));
            out.insert("report".into(), json!(report));
            out.insert("credit".into(), credit);
            out.insert(
                "streak_days".into(),
                streak_days.unwrap_or(Value::Null),
            );
            out.insert(
                "total_credits".into(),
                total_credits.unwrap_or(Value::Null),
            );
            out.insert(
                "is_streak_day".into(),
                is_streak_day.unwrap_or(Value::Null),
            );
            out.insert(
                "next_streak_day".into(),
                next_streak_day.unwrap_or(Value::Null),
            );

            return ServiceRun {
                code: 0,
                out: Value::Object(out),
            };
        }

        if claim
            .body
            .as_object()
            .is_some_and(|object| object.contains_key("code") || object.contains_key("msg"))
        {
            let msg = dig(&claim.body, "msg")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| dig(&claim.body, "code").map(display_value))
                .unwrap_or_else(|| "unknown".to_string());

            return ServiceRun {
                code: 1,
                out: json!({
                    "result": "ERROR",
                    "report": format!("领取失败：{msg}（HTTP {}）", claim.code),
                    "http": claim.code,
                    "claim_body": claim.body
                }),
            };
        }

        let preview: String = claim.body.to_string().chars().take(200).collect();
        ServiceRun {
            code: 1,
            out: json!({
                "result": "UNKNOWN",
                "report": format!("未识别的领取返回，请检查接口：{preview}"),
                "http": claim.code,
                "claim_body": claim.body
            }),
        }
    }
}

pub fn is_already_checked_in(body: &Value) -> bool {
    if body.is_null() {
        return true;
    }

    let Some(object) = body.as_object() else {
        return false;
    };

    if object.get("code").and_then(Value::as_i64) == Some(10001) {
        return true;
    }

    object
        .get("msg")
        .and_then(Value::as_str)
        .is_some_and(|message| message.contains("已签"))
}

pub fn already_report(status: &Value, via: Option<&str>) -> Value {
    let today_credit = dig(status, "today_credit")
        .cloned()
        .or_else(|| dig(status, "daily_credit").cloned());
    let streak_days = dig(status, "streak_days").cloned();
    let total_credits = dig(status, "total_credits").cloned();
    let is_streak_day = dig(status, "is_streak_day").cloned();
    let next_streak_day = dig(status, "next_streak_day").cloned();

    let mut inner = Vec::new();
    if let Some(value) = today_credit.as_ref() {
        inner.push(format!("今日 +{}", format_credit(value)));
    }
    if let Some(value) = streak_days.as_ref() {
        inner.push(format!("连续 {} 天", display_value(value)));
    }
    if let Some(value) = total_credits.as_ref() {
        inner.push(format!("累计 {} 积分", format_credit(value)));
    }

    let prefix = via.unwrap_or("今日已签过");
    let report = if inner.is_empty() {
        prefix.to_string()
    } else {
        format!("{}（{}）", prefix, inner.join("，"))
    };

    json!({
        "result": "ALREADY",
        "report": report,
        "today_credit": today_credit.unwrap_or(Value::Null),
        "streak_days": streak_days.unwrap_or(Value::Null),
        "total_credits": total_credits.unwrap_or(Value::Null),
        "is_streak_day": is_streak_day.unwrap_or(Value::Null),
        "next_streak_day": next_streak_day.unwrap_or(Value::Null)
    })
}

fn no_session_with_http(code: i32) -> ServiceRun {
    ServiceRun {
        code: 1,
        out: json!({
            "result": "NO_SESSION",
            "report": format!(
                "登录态已失效（HTTP {code}），请重新登录 WorkBuddy 桌面端"
            ),
            "http": code
        }),
    }
}

pub fn display_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}
