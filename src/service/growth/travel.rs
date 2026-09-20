use serde_json::Value;

use crate::http::{CODE_BUDGET_OUT, CODE_NO_NETWORK};
use crate::model::common::ServiceRun;
use crate::service::signin::display_value;
use crate::util::{as_i64, dig, format_eta, value_truthy};

use super::context::{check_auth, message_or_http, no_session, GrowthAccumulator, GrowthContext};

pub async fn run(ctx: &GrowthContext<'_>, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
    let status = ctx.api.travel_status().await;

    if status.code == CODE_BUDGET_OUT {
        return Some(ServiceRun {
            code: 1,
            out: serde_json::json!({
                "result": "TIMEOUT",
                "report": "时间预算耗尽，成长中心跳过，下次自动重试"
            }),
        });
    }

    if status.code == CODE_NO_NETWORK {
        let error = dig(&status.body, "error")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Some(ServiceRun {
            code: 1,
            out: serde_json::json!({
                "result": "NETWORK",
                "report": format!("网络不可达，成长中心跳过（{error}）")
            }),
        });
    }

    if check_auth(status.code) {
        return Some(no_session());
    }

    let mut travel = if status.is_success() {
        dig(&status.body, "state")
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        None
    };

    let daily_limit = status.is_success() && value_truthy(dig(&status.body, "daily_limit_reached"));

    acc.note_http(&status, "查旅行状态");

    if travel.as_deref() == Some("arrived") {
        let record_id = dig(&status.body, "record_id")
            .cloned()
            .unwrap_or(Value::Null);
        let claim = ctx.api.travel_claim(record_id).await;

        if check_auth(claim.code) {
            return Some(no_session());
        }

        if claim.is_success() && dig(&claim.body, "reward_credit").is_some() {
            let got = as_i64(dig(&claim.body, "reward_credit"), 0);
            acc.credits_gained += got;
            acc.parts.push(format!("领旅行礼物 +{got} 积分"));
            acc.successes += 1;
            travel = Some("idle".to_string());
        } else {
            acc.record_failure(
                claim.code,
                format!(
                    "领旅行礼物失败：{}",
                    message_or_http(&claim.body, claim.code)
                ),
            );
        }
    }

    if travel.as_deref() == Some("idle") && daily_limit {
        acc.parts.push("今日旅行名额已用完".to_string());
    } else if travel.as_deref() == Some("idle") {
        let config = ctx.api.travel_config().await;

        if check_auth(config.code) {
            return Some(no_session());
        }

        if let Some(location) = dig(&config.body, "locations")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_object)
        {
            let location_id = location.get("id").cloned().unwrap_or(Value::Null);
            let depart = ctx.api.travel_depart(location_id).await;

            if check_auth(depart.code) {
                return Some(no_session());
            }

            if depart.is_success() {
                let location_name = dig(&depart.body, "location")
                    .and_then(Value::as_object)
                    .and_then(|object| object.get("name"))
                    .map(display_value)
                    .unwrap_or_else(|| "?".to_string());

                let duration = dig(&depart.body, "duration_hours")
                    .cloned()
                    .or_else(|| {
                        dig(&depart.body, "location")
                            .and_then(Value::as_object)
                            .and_then(|object| object.get("duration_hours"))
                            .cloned()
                    })
                    .map(|value| display_value(&value))
                    .unwrap_or_else(|| "?".to_string());

                acc.parts
                    .push(format!("派 Buddy 去{location_name}（{duration} 小时后回）"));
                acc.successes += 1;
            } else {
                acc.record_failure(
                    depart.code,
                    format!(
                        "派 Buddy 失败：{}",
                        message_or_http(&depart.body, depart.code)
                    ),
                );
            }
        }
    } else if travel.as_deref() == Some("traveling") {
        let location_name = dig(&status.body, "location")
            .and_then(Value::as_object)
            .and_then(|object| object.get("name"))
            .map(display_value)
            .unwrap_or_else(|| "?".to_string());

        let eta = format_eta(
            dig(&status.body, "arrive_at"),
            dig(&status.body, "server_now"),
        );

        acc.parts
            .push(format!("Buddy 旅行中（{location_name}{eta}）"));
    }

    None
}
