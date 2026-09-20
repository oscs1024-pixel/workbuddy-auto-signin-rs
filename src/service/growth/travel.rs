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

    if acc.note_http(&status, "查旅行状态") {
        return None;
    }

    let mut travel = match dig(&status.body, "state").and_then(Value::as_str) {
        Some(state @ ("idle" | "arrived" | "traveling")) => Some(state.to_string()),
        Some(state) => {
            acc.record_schema_mismatch("查旅行状态", format!("未知 state={state:?}"));
            return None;
        }
        None => {
            acc.record_schema_mismatch("查旅行状态", "缺少字符串字段 state");
            return None;
        }
    };

    let daily_limit = value_truthy(dig(&status.body, "daily_limit_reached"));

    if travel.as_deref() == Some("arrived") {
        let Some(record_id) = dig(&status.body, "record_id")
            .filter(|value| !value.is_null())
            .cloned()
        else {
            acc.record_schema_mismatch("查旅行状态", "arrived 状态缺少 record_id");
            return None;
        };
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

        // config 属于旅行状态机的必要读步骤；最终失败不能静默吞掉，否则整轮可能
        // 被误判为 idle，导致 silent-poll 把真实服务端/网络故障隐藏掉。
        if acc.note_http(&config, "查旅行配置") {
            return None;
        }

        let Some(locations) = dig(&config.body, "locations").and_then(Value::as_array) else {
            acc.record_schema_mismatch("查旅行配置", "缺少数组字段 locations");
            return None;
        };

        if let Some(location_value) = locations.first() {
            let Some(location) = location_value.as_object() else {
                acc.record_schema_mismatch("查旅行配置", "locations[0] 不是对象");
                return None;
            };
            let Some(location_id) = location.get("id").filter(|value| !value.is_null()).cloned()
            else {
                acc.record_schema_mismatch("查旅行配置", "locations[0] 缺少 id");
                return None;
            };
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

                let message = if matches!(duration.as_str(), "0" | "0.0") {
                    format!("Buddy 已前往{location_name}（即将返回）")
                } else if duration == "?" {
                    format!("Buddy 已前往{location_name}")
                } else {
                    format!("Buddy 已前往{location_name}（约 {duration} 小时后返回）")
                };
                acc.parts.push(message);
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

        let arrive_at = dig(&status.body, "arrive_at");
        let server_now = dig(&status.body, "server_now");
        if arrive_at.is_none() || server_now.is_none() {
            acc.record_schema_mismatch(
                "查旅行状态",
                "traveling 状态缺少 arrive_at 或 server_now",
            );
        }
        let eta = format_eta(arrive_at, server_now);

        acc.parts
            .push(format!("Buddy 旅行中（{location_name}{eta}）"));
    }

    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use reqwest::header::HeaderMap;
    use serde_json::json;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::api::GrowthApi;
    use crate::budget::Budget;
    use crate::http::WorkBuddyClient;

    use super::*;
    use crate::service::growth::context::{GrowthAccumulator, GrowthContext};

    fn api(server: &MockServer, budget: Arc<Budget>) -> GrowthApi {
        let client =
            WorkBuddyClient::new(Url::parse(&server.uri()).unwrap(), HeaderMap::new(), budget)
                .unwrap();
        GrowthApi::new(client)
    }

    #[tokio::test]
    async fn travel_config_hard_failure_is_recorded() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/activity/growth/buddy/travel/status"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data":{"state":"idle","daily_limit_reached":false}})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v2/activity/growth/buddy/travel/config"))
            .respond_with(ResponseTemplate::new(503).set_body_json(json!({"msg":"unavailable"})))
            .expect(1)
            .mount(&server)
            .await;

        // 预算小于 3s 退避 + 30s 请求上限，因此 503 不会进入真实 sleep 重试。
        let budget = Arc::new(Budget::with_limit(Duration::from_secs(5)));
        let api = api(&server, budget.clone());
        let ctx = GrowthContext {
            api: &api,
            budget: &budget,
        };
        let mut acc = GrowthAccumulator::default();

        assert!(run(&ctx, &mut acc).await.is_none());
        assert_eq!(acc.failures, 1);
        assert_eq!(acc.hard_failures, 1);
        assert!(acc.parts.iter().any(|part| part.contains("查旅行配置失败")));
    }

    #[tokio::test]
    async fn failed_travel_claim_never_departs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/activity/growth/buddy/travel/status"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data":{"state":"arrived","record_id":"r1"}})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/activity/growth/buddy/travel/claim"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"msg":"claim failed"})))
            .expect(1)
            .mount(&server)
            .await;

        let budget = Arc::new(Budget::with_limit(Duration::from_secs(5)));
        let api = api(&server, budget.clone());
        let ctx = GrowthContext {
            api: &api,
            budget: &budget,
        };
        let mut acc = GrowthAccumulator::default();

        assert!(run(&ctx, &mut acc).await.is_none());
        assert_eq!(acc.failures, 1);

        let requests = server.received_requests().await.unwrap();
        let paths: Vec<_> = requests.iter().map(|request| request.url.path()).collect();
        assert_eq!(
            paths,
            vec![
                "/v2/activity/growth/buddy/travel/status",
                "/v2/activity/growth/buddy/travel/claim"
            ]
        );
    }
}
