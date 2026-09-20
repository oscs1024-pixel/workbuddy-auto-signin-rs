use serde_json::Value;

use crate::model::common::ServiceRun;
use crate::util::{client_token, dig, try_i64, value_truthy};

use super::context::{check_auth, message_or_http, no_session, GrowthAccumulator, GrowthContext};

pub async fn run(ctx: &GrowthContext<'_>, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
    if ctx.budget.exhausted() {
        acc.record_budget_exhausted("时间预算耗尽，盲盒跳过");
        return None;
    }

    let chances_response = ctx.api.lottery_chances().await;

    if check_auth(chances_response.code) {
        return Some(no_session());
    }

    if acc.note_http(&chances_response, "查抽奖机会") {
        return None;
    }

    let Some(chances) = try_i64(dig(&chances_response.body, "balance")) else {
        acc.record_schema_mismatch("查抽奖机会", "缺少或无法解析 balance");
        return None;
    };

    if chances <= 0 {
        return None;
    }

    let draw = ctx.api.lottery_draw(client_token("u")).await;

    if check_auth(draw.code) {
        return Some(no_session());
    }

    if draw.is_success() {
        let prize_value = dig(&draw.body, "prize_name").or_else(|| dig(&draw.body, "prize"));

        let mut prize = match prize_value {
            Some(Value::String(value)) => value.clone(),
            Some(other) => other.to_string(),
            None => {
                acc.record_schema_mismatch("抽奖结果", "缺少 prize_name/prize");
                "未知".to_string()
            }
        };

        if value_truthy(dig(&draw.body, "need_address"))
            || value_truthy(dig(&draw.body, "require_address"))
        {
            prize.push_str("（实物奖，需到成长中心填写收件信息）");
        }

        acc.parts.push(format!("开盲盒获得：{prize}"));
        acc.successes += 1;

        if chances > 1 {
            acc.parts
                .push(format!("还剩 {} 次抽奖机会，下轮继续", chances - 1));
        }
    } else {
        let message = dig(&draw.body, "msg").and_then(Value::as_str).unwrap_or("");

        if is_no_chance(message) {
            acc.parts.push(format!(
                "开盲盒：{}",
                if message.is_empty() {
                    "无抽奖机会"
                } else {
                    message
                }
            ));
        } else {
            acc.record_failure(
                draw.code,
                format!("开盲盒失败：{}", message_or_http(&draw.body, draw.code)),
            );
        }
    }

    None
}

pub fn is_no_chance(message: &str) -> bool {
    let message = message.to_ascii_lowercase();

    if message.is_empty() {
        return false;
    }

    if message.contains("insufficient") || message.contains("not enough") {
        return message.contains("chance") || message.contains("balance");
    }

    message.contains("no chance")
}

#[cfg(test)]
mod state_tests {
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

    #[tokio::test]
    async fn draws_only_once_even_with_multiple_chances() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/activity/growth/lottery/chances"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"balance":3}})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/activity/growth/lottery/draw"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"data":{"prize_name":"积分"}})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let budget = Arc::new(Budget::with_limit(Duration::from_secs(5)));
        let client = WorkBuddyClient::new(
            Url::parse(&server.uri()).unwrap(),
            HeaderMap::new(),
            budget.clone(),
        )
        .unwrap();
        let api = GrowthApi::new(client);
        let ctx = GrowthContext {
            api: &api,
            budget: &budget,
        };
        let mut acc = GrowthAccumulator::default();

        assert!(run(&ctx, &mut acc).await.is_none());
        assert_eq!(acc.successes, 1);
        assert!(acc.parts.iter().any(|part| part.contains("还剩 2 次")));
    }
}
