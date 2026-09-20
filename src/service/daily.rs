use serde_json::{json, Value};

use crate::model::common::ServiceRun;
use crate::util::as_i64;

use super::growth::GrowthService;
use super::signin::SigninService;

pub async fn run_daily(signin: &SigninService, growth: &GrowthService) -> (i32, Value, bool) {
    let ServiceRun { mut code, mut out } = signin.run().await;

    let result = out
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    if matches!(result.as_str(), "NETWORK" | "TIMEOUT") {
        insert(
            &mut out,
            "growth",
            json!("网络不可达或时间预算耗尽，成长中心跳过"),
        );
        insert(&mut out, "growth_result", json!(result));
        return (code, out, false);
    }

    if result == "NO_SESSION" {
        insert(&mut out, "growth", json!("登录态已失效，成长中心跳过"));
        insert(&mut out, "growth_result", json!("NO_SESSION"));
        return (code, out, false);
    }

    let growth_run = growth.run().await;
    let growth_report = growth_run.out.get("report").cloned().unwrap_or(Value::Null);
    let growth_result = growth_run.out.get("result").cloned().unwrap_or(Value::Null);

    insert(&mut out, "growth", growth_report.clone());
    insert(&mut out, "growth_result", growth_result.clone());

    // 给交互式输出保留结构化成长中心结果；同时带上 result/report，
    // 避免成长中心提前返回 NETWORK/NO_SESSION/TIMEOUT 时被误显示成“无可处理项目”。
    let growth_detail = json!({
        "result": growth_result,
        "report": growth_report.clone(),
        "items": growth_run.out.get("items").cloned().unwrap_or_else(|| json!([])),
        "energy": growth_run.out.get("energy").cloned().unwrap_or(Value::Null),
        "streak_days": growth_run.out.get("streak_days").cloned().unwrap_or(Value::Null),
        "credits_gained": growth_run
            .out
            .get("credits_gained")
            .cloned()
            .unwrap_or_else(|| json!(0)),
        "idle": growth_run.out.get("idle").cloned().unwrap_or(Value::Bool(false)),
        "failures": growth_run.out.get("failures").cloned().unwrap_or_else(|| json!(0))
    });
    insert(&mut out, "growth_detail", growth_detail);

    let credits_gained = as_i64(growth_run.out.get("credits_gained"), 0);
    if credits_gained != 0 {
        if let (Some(main_report), Some(extra)) = (
            out.get("report").and_then(Value::as_str),
            growth_report.as_str(),
        ) {
            let combined = format!("{main_report}；{extra}");
            insert(&mut out, "report", json!(combined));
        }
    }

    if growth_run.code != 0 && code == 0 {
        code = growth_run.code;
    }

    let idle = growth_run
        .out
        .get("idle")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let quiet = is_quiet(&result, idle);

    (code, out, quiet)
}

pub fn is_quiet(signin_result: &str, growth_idle: bool) -> bool {
    matches!(signin_result, "ALREADY" | "INACTIVE") && growth_idle
}

fn insert(out: &mut Value, key: &str, value: Value) {
    if let Some(object) = out.as_object_mut() {
        object.insert(key.to_string(), value);
    }
}
