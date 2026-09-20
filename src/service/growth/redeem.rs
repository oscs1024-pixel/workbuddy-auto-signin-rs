use serde_json::{json, Value};

use crate::http::HttpResult;
use crate::model::common::ServiceRun;
use crate::util::{as_i64, client_token, dig, value_string};

use super::context::{check_auth, message_or_http, no_session, GrowthAccumulator, GrowthContext};

const REDEEM_TIERS: &[(&str, &str, &str, i64)] = &[
    ("7d", "starter", "入门", 7),
    ("14d", "advanced", "进阶", 14),
    ("28d", "legendary", "巅峰", 28),
];

pub async fn run(ctx: &GrowthContext<'_>, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
    if ctx.budget.exhausted() {
        acc.record_budget_exhausted("时间预算耗尽，连登兑换跳过");
        return None;
    }

    let summary = ctx.api.redeem_summary().await;

    if check_auth(summary.code) {
        return Some(no_session());
    }

    if acc.note_http(&summary, "查连登兑换") {
        return None;
    }

    let known_statuses = REDEEM_TIERS
        .iter()
        .filter(|(_, status_key, _, _)| {
            let key = format!("{status_key}_status");
            dig(&summary.body, &key).is_some()
        })
        .count();
    if known_statuses == 0 {
        acc.record_schema_mismatch(
            "查连登兑换",
            "缺少 starter/advanced/legendary_status",
        );
        return None;
    }

    for (tier, status_key, label, days) in REDEEM_TIERS {
        if ctx.budget.exhausted() {
            acc.record_budget_exhausted("时间预算耗尽，剩余连登兑换下次再领");
            break;
        }

        let key = format!("{status_key}_status");
        let Some(status_value) = dig(&summary.body, &key) else {
            continue;
        };
        let Some(status) = status_value.as_str() else {
            acc.record_schema_mismatch("查连登兑换", format!("{key} 不是字符串"));
            continue;
        };

        if status.is_empty() || matches!(status, "claimed" | "locked") {
            continue;
        }

        // 主契约使用 7d/14d/28d；只有服务端明确表示“不认识 tier”时才退回数字天数重试。
        let mut redeemed = ctx.api.redeem(json!(tier), client_token("u")).await;

        if is_unknown_tier(&redeemed) {
            redeemed = ctx.api.redeem(json!(days), client_token("u")).await;
        }

        // 403“连登天数不足”是业务常态，必须先于通用 401/403 登录失效判断。
        if is_tier_locked(&redeemed) {
            acc.parts
                .push(format!("连登兑换「{label}」未解锁（连登天数不足）"));
            continue;
        }

        if check_auth(redeemed.code) {
            return Some(no_session());
        }

        if redeemed.is_success() {
            acc.credits_gained += as_i64(dig(&redeemed.body, "credit_granted"), 0);
            acc.parts.push(format!(
                "连登兑换「{label}」{}",
                redeem_reward_desc(&redeemed.body, tier)
            ));
            acc.successes += 1;
        } else {
            acc.record_failure(
                redeemed.code,
                format!(
                    "连登兑换「{label}」失败：{}",
                    message_or_http(&redeemed.body, redeemed.code)
                ),
            );
        }
    }

    None
}

pub fn is_unknown_tier(response: &HttpResult) -> bool {
    if response.code != 400 {
        return false;
    }

    let message = value_string(dig(&response.body, "msg"))
        .unwrap_or_default()
        .to_ascii_lowercase();

    message.contains("tier")
        && (message.contains("unknown")
            || message.contains("unsupported")
            || message.contains("invalid"))
}

pub fn is_tier_locked(response: &HttpResult) -> bool {
    if response.code != 403 {
        return false;
    }

    let message = value_string(dig(&response.body, "msg")).unwrap_or_default();

    message.contains("天数不足")
        || (message.contains("不足")
            && (message.contains("连登")
                || message.contains("连续登录")
                || message.contains("连续签到")))
}

pub fn redeem_reward_desc(body: &Value, tier: &str) -> String {
    let credit = as_i64(dig(body, "credit_granted"), 0);
    let energy = as_i64(dig(body, "energy_granted"), 0);
    let cards = as_i64(dig(body, "cards_granted"), 0);
    let chances = as_i64(dig(body, "chances_granted"), 0);

    let mut bits = Vec::new();
    if credit != 0 {
        bits.push(format!("+{credit} 积分"));
    }
    if energy != 0 {
        bits.push(format!("+{energy} 能量"));
    }
    if cards != 0 {
        bits.push(format!("+{cards} 补登卡"));
    }
    if chances != 0 {
        bits.push(format!("+{chances} 次抽奖"));
    }

    if !bits.is_empty() {
        return format!("（{}）", bits.join(" "));
    }

    // 服务端未返回 *_granted 时才使用活动文案兜底，避免把配置奖励误当成实际到账值。
    let fallback = match tier {
        "7d" => "+2 能量 +1 补登卡 +1 次抽奖",
        "14d" => "+50 积分 +3 能量 +1 补登卡 +1 次抽奖",
        "28d" => "+150 积分 +5 能量 +1 补登卡 +1 次抽奖",
        _ => "奖励已到账",
    };

    format!("（{fallback}）")
}

#[cfg(test)]
mod state_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use reqwest::header::HeaderMap;
    use serde_json::{json, Value};
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

    use crate::api::GrowthApi;
    use crate::budget::Budget;
    use crate::http::WorkBuddyClient;

    use super::*;
    use crate::service::growth::context::{GrowthAccumulator, GrowthContext};

    #[derive(Clone)]
    struct Tier(Value);

    impl Match for Tier {
        fn matches(&self, request: &Request) -> bool {
            serde_json::from_slice::<Value>(&request.body)
                .ok()
                .and_then(|body| body.get("tier").cloned())
                .is_some_and(|tier| tier == self.0)
        }
    }

    fn setup(server: &MockServer, budget: Arc<Budget>) -> (GrowthApi, Arc<Budget>) {
        let client = WorkBuddyClient::new(
            Url::parse(&server.uri()).unwrap(),
            HeaderMap::new(),
            budget.clone(),
        )
        .unwrap();
        (GrowthApi::new(client), budget)
    }

    #[tokio::test]
    async fn fallback_redeem_uses_a_fresh_client_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/activity/growth/redeem/summary"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "starter_status": "available",
                    "advanced_status": "locked",
                    "legendary_status": "locked"
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/activity/growth/redeem"))
            .and(Tier(json!("7d")))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({"msg":"unknown tier"})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/activity/growth/redeem"))
            .and(Tier(json!(7)))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"data":{"energy_granted":2}})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let budget = Arc::new(Budget::with_limit(Duration::from_secs(5)));
        let (api, budget) = setup(&server, budget);
        let ctx = GrowthContext {
            api: &api,
            budget: &budget,
        };
        let mut acc = GrowthAccumulator::default();

        assert!(run(&ctx, &mut acc).await.is_none());
        assert_eq!(acc.successes, 1);

        let requests = server.received_requests().await.unwrap();
        let redeem_requests: Vec<_> = requests
            .iter()
            .filter(|request| request.url.path() == "/v2/activity/growth/redeem")
            .collect();
        assert_eq!(redeem_requests.len(), 2);

        let first: Value = serde_json::from_slice(&redeem_requests[0].body).unwrap();
        let second: Value = serde_json::from_slice(&redeem_requests[1].body).unwrap();
        assert_ne!(first["client_token"], second["client_token"]);
    }

    #[tokio::test]
    async fn locked_tier_is_business_state_not_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/activity/growth/redeem/summary"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "starter_status": "available",
                    "advanced_status": "locked",
                    "legendary_status": "locked"
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/activity/growth/redeem"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(json!({"msg":"连续登录天数不足"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let budget = Arc::new(Budget::with_limit(Duration::from_secs(5)));
        let (api, budget) = setup(&server, budget);
        let ctx = GrowthContext {
            api: &api,
            budget: &budget,
        };
        let mut acc = GrowthAccumulator::default();

        assert!(run(&ctx, &mut acc).await.is_none());
        assert_eq!(acc.failures, 0);
        assert!(acc.parts.iter().any(|part| part.contains("未解锁")));
    }
}
