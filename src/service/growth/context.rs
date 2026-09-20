use serde_json::Value;

use crate::api::GrowthApi;
use crate::budget::Budget;
use crate::http::{http_label, is_hard_failure, HttpResult};
use crate::model::common::ServiceRun;
use crate::service::signin::display_value;

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
    pub successes: usize,
}

impl GrowthAccumulator {
    pub fn note_http(&mut self, response: &HttpResult, label: &str) -> bool {
        if response.is_success() {
            return false;
        }

        let reason = http_label(response.code);
        let detail = response
            .body
            .as_object()
            .and_then(|object| object.get("error").or_else(|| object.get("msg")))
            .map(display_value)
            .unwrap_or_default();

        let description = if !detail.is_empty() && detail != reason {
            format!("{reason}（{detail}）")
        } else if !detail.is_empty() {
            detail
        } else {
            reason
        };

        self.parts.push(format!("{label}失败：{description}"));

        if is_hard_failure(response.code) {
            self.failures += 1;
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
        if let Some(detail) = crate::util::dig(body, key)
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

    use super::message_or_http;
    use crate::http::CODE_NO_NETWORK;

    #[test]
    fn message_or_http_keeps_network_error_detail() {
        assert_eq!(
            message_or_http(&json!({"error": "connection reset by peer"}), CODE_NO_NETWORK),
            "connection reset by peer"
        );
        assert_eq!(message_or_http(&json!({}), 503), "HTTP 503");
    }
}
