use serde_json::{json, Map, Value};

use crate::model::common::ServiceRun;
use crate::service::signin::display_value;
use crate::util::dig;

use super::context::{check_auth, GrowthAccumulator, GrowthContext};

pub async fn load_values(
    ctx: &GrowthContext<'_>,
    streak_body: Option<Value>,
    streak_stale: bool,
) -> (Option<Value>, Option<Value>) {
    let mut energy = None;

    if !ctx.budget.exhausted() {
        let response = ctx.api.energy().await;
        if !check_auth(response.code) && response.is_success() {
            energy = dig(&response.body, "balance").cloned();
        }
    }

    let streak_days = if let Some(body) = streak_body.filter(|_| !streak_stale) {
        dig(&body, "streak")
            .and_then(Value::as_object)
            .and_then(|object| object.get("days"))
            .cloned()
    } else if !ctx.budget.exhausted() {
        let response = ctx.api.streak().await;

        if !check_auth(response.code) {
            dig(&response.body, "streak")
                .and_then(Value::as_object)
                .and_then(|object| object.get("days"))
                .cloned()
        } else {
            None
        }
    } else {
        None
    };

    (energy, streak_days)
}

pub fn finalize(
    acc: GrowthAccumulator,
    energy: Option<Value>,
    streak_days: Option<Value>,
) -> ServiceRun {
    let mut tail = Vec::new();

    if let Some(value) = energy.as_ref() {
        tail.push(format!("能量 {}", display_value(value)));
    }

    if let Some(value) = streak_days.as_ref() {
        tail.push(format!("连签 {} 天", display_value(value)));
    }

    if acc.credits_gained != 0 {
        tail.push(format!("本次共 +{} 积分", acc.credits_gained));
    }

    // 同时保留结构化步骤，终端展示层无需再从长字符串里反向拆分业务结果。
    let items = acc.parts.clone();
    let mut report = if !items.is_empty() {
        items.join("；")
    } else if acc.failures > 0 {
        "成长中心各步骤均失败".to_string()
    } else {
        "成长中心无可领取项".to_string()
    };

    if !tail.is_empty() {
        report.push_str(&format!("（{}）", tail.join("，")));
    }

    let code = if acc.hard_failures > 0 && acc.successes == 0 {
        1
    } else {
        0
    };

    let idle = acc.successes == 0 && acc.failures == 0;

    let mut out = Map::new();
    out.insert("result".into(), json!("GROWTH"));
    out.insert("report".into(), json!(report));
    out.insert("items".into(), json!(items));
    out.insert("credits_gained".into(), json!(acc.credits_gained));
    out.insert("energy".into(), energy.unwrap_or(Value::Null));
    out.insert("streak_days".into(), streak_days.unwrap_or(Value::Null));
    out.insert("idle".into(), json!(idle));

    if acc.failures > 0 {
        out.insert("failures".into(), json!(acc.failures));
    }
    if acc.hard_failures > 0 {
        out.insert("hard_failures".into(), json!(acc.hard_failures));
    }
    if acc.schema_mismatches > 0 {
        out.insert("schema_mismatches".into(), json!(acc.schema_mismatches));
    }

    ServiceRun {
        code,
        out: Value::Object(out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResult;

    #[test]
    fn soft_failure_is_not_idle_and_does_not_fail_exit_code() {
        let mut acc = GrowthAccumulator::default();
        acc.note_http(
            &HttpResult {
                code: 404,
                body: json!({"msg":"endpoint moved"}),
            },
            "查任务列表",
        );

        let run = finalize(acc, None, None);
        assert_eq!(run.code, 0);
        assert_eq!(run.out["idle"], false);
        assert_eq!(run.out["failures"], 1);
    }
}
