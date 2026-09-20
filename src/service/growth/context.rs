use serde_json::Value;

use crate::api::GrowthApi;
use crate::budget::Budget;
use crate::http::{http_label, is_hard_failure, HttpResult, CODE_BUDGET_OUT};
use crate::model::common::ServiceRun;
use crate::service::signin::display_value;
use crate::util::dig;

pub struct GrowthContext<'a> {
    pub api: &'a GrowthApi,
    pub budget: &'a Budget,
}

#[derive(Debug, Default)]
pub struct GrowthAccumulator {
    pub parts: Vec<String>,
    pub credits_gained: i64,
    pub failures: usize,
    pub hard_failures: usize,
    pub schema_mismatches: usize,
    pub successes: usize,
    budget_exhausted_reported: bool,
}

impl GrowthAccumulator {
    pub fn note_http(&mut self, response: &HttpResult, label: &str) -> bool {
        if response.is_success() {
            return false;
        }

        let reason = http_label(response.code);
        let detail = ["error", "msg"]
            .iter()
            .find_map(|key| dig(&response.body, key).map(display_value))
            .unwrap_or_default();

        let description = if !detail.is_empty() && detail != reason {
            format!("{reason}（{detail}）")
        } else if !detail.is_empty() {
            detail
        } else {
            reason
        };

        self.parts.push(format!("{label}失败：{description}"));
        // failures 决定这一轮是否属于真正 idle；hard_failures 单独决定退出码。
        // 因此读接口 4xx 也必须阻止 silent-poll 静默，但不会把进程标成硬失败。
        self.failures += 1;
        if is_hard_failure(response.code) {
            self.hard_failures += 1;
        }

        true
    }

    pub fn record_failure(&mut self, code: i32, message: String) {
        self.parts.push(message);
        self.failures += 1;
        if is_hard_failure(code) {
            self.hard_failures += 1;
        }
    }

    pub fn record_schema_mismatch(&mut self, label: &str, detail: impl AsRef<str>) {
        self.parts
            .push(format!("{label}响应结构异常：{}", detail.as_ref()));
        self.failures += 1;
        self.schema_mismatches += 1;
    }

    pub fn record_budget_exhausted(&mut self, message: impl Into<String>) {
        if self.budget_exhausted_reported {
            return;
        }
        self.budget_exhausted_reported = true;
        self.record_failure(CODE_BUDGET_OUT, message.into());
    }
}

pub fn check_auth(code: i32) -> bool {
    matches!(code, 401 | 403)
}

pub fn no_session() -> ServiceRun {
    ServiceRun {
        code: 1,
        out: serde_json::json!({
            "result": "NO_SESSION",
            "report": "登录态已失效，请重新登录 WorkBuddy 桌面端"
        }),
    }
}

pub fn message_or_http(body: &Value, code: i32) -> String {
    // HTTP 业务失败通常给 msg；网络/超时伪状态码则由客户端写入 error。
    // 两者都保留，避免 Growth 写请求只显示 "HTTP -1" 而丢失真正原因。
    for key in ["msg", "error"] {
        if let Some(detail) = dig(body, key)
            .map(display_value)
            .filter(|detail| !detail.is_empty())
        {
            return detail;
        }
    }

    http_label(code)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{message_or_http, GrowthAccumulator};
    use crate::http::{HttpResult, CODE_NO_NETWORK};

    #[test]
    fn message_or_http_keeps_network_error_detail() {
        assert_eq!(
            message_or_http(
                &json!({"error": "connection reset by peer"}),
                CODE_NO_NETWORK
            ),
            "connection reset by peer"
        );
        assert_eq!(message_or_http(&json!({}), 503), "HTTP 503");
    }

    #[test]
    fn soft_http_failure_prevents_idle_without_becoming_hard_failure() {
        let mut acc = GrowthAccumulator::default();
        let response = HttpResult {
            code: 404,
            body: json!({"msg": "endpoint moved"}),
        };

        assert!(acc.note_http(&response, "查任务列表"));
        assert_eq!(acc.failures, 1);
        assert_eq!(acc.hard_failures, 0);
    }

    #[test]
    fn schema_mismatch_is_observable_but_not_hard_failure() {
        let mut acc = GrowthAccumulator::default();
        acc.record_schema_mismatch("查任务列表", "缺少 tasks 数组");

        assert_eq!(acc.failures, 1);
        assert_eq!(acc.schema_mismatches, 1);
        assert_eq!(acc.hard_failures, 0);
    }
}
