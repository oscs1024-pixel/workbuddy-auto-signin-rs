use serde_json::Value;

use crate::config::MAKEUP_MAX_PER_RUN;
use crate::model::common::ServiceRun;
use crate::service::signin::display_value;
use crate::util::{as_i64, client_token, dig, try_i64};

use super::context::{check_auth, message_or_http, no_session, GrowthAccumulator, GrowthContext};

pub struct MakeupState {
    pub streak_body: Option<Value>,
    pub streak_stale: bool,
}

pub fn parse_makeup_cards(value: Option<&Value>) -> i64 {
    match value.and_then(Value::as_object) {
        Some(object) => as_i64(object.get("balance"), 0),
        None => as_i64(value, 0),
    }
}

pub async fn run(
    ctx: &GrowthContext<'_>,
    acc: &mut GrowthAccumulator,
) -> Result<MakeupState, ServiceRun> {
    if ctx.budget.exhausted() {
        acc.record_budget_exhausted("时间预算耗尽，补登跳过");
        return Ok(MakeupState {
            streak_body: None,
            streak_stale: false,
        });
    }

    let response = ctx.api.streak().await;

    if check_auth(response.code) {
        return Err(no_session());
    }

    if acc.note_http(&response, "查连登状态") {
        return Ok(MakeupState {
            streak_body: None,
            streak_stale: false,
        });
    }

    let streak_body = Some(response.body.clone());
    let Some(streak_object) = dig(&response.body, "streak").and_then(Value::as_object) else {
        acc.record_schema_mismatch("查连登状态", "缺少对象字段 streak");
        return Ok(MakeupState {
            streak_body,
            streak_stale: false,
        });
    };
    if streak_object.get("days").is_none() {
        acc.record_schema_mismatch("查连登状态", "streak 缺少 days");
    }

    let cards_value = dig(&response.body, "makeup_cards");
    if cards_value.is_none() {
        acc.record_schema_mismatch("查连登状态", "缺少 makeup_cards");
    } else {
        let numeric = cards_value
            .and_then(Value::as_object)
            .and_then(|object| object.get("balance"))
            .or(cards_value);
        if try_i64(numeric).is_none() {
            acc.record_schema_mismatch("查连登状态", "makeup_cards 无法解析为余额");
        }
    }
    let mut cards = parse_makeup_cards(cards_value);

    let nested_dates = Some(streak_object)
        .and_then(|object| object.get("makeup_dates"))
        .and_then(Value::as_array);

    let dates: Vec<Value> = match nested_dates {
        Some(items) if !items.is_empty() => items.clone(),
        _ => dig(&response.body, "makeup_dates")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };

    let mut streak_stale = false;

    if cards > 0 && !dates.is_empty() {
        let mut cards_used = 0usize;
        let mut dates_checked = 0usize;

        for date in &dates {
            // 限制的是“实际消耗的补登卡”而不是“探测过的日期”。
            // 服务端偶尔会在 makeup_dates 中残留已经无需补登的旧日期；若它挡在首位，
            // 每轮只检查第一项会让后面的真实断登日期永远得不到处理。
            if cards <= 0 || cards_used >= MAKEUP_MAX_PER_RUN {
                break;
            }
            if ctx.budget.exhausted() {
                acc.record_budget_exhausted("时间预算耗尽，剩余补登下次再做");
                break;
            }

            dates_checked += 1;
            let used = ctx
                .api
                .use_makeup_card(date.clone(), client_token("u"))
                .await;

            // 登录失效必须优先于业务文案判断，避免 401/403 被“无需补登”等文本误吞。
            if check_auth(used.code) {
                return Err(no_session());
            }

            let message = message_or_http(&used.body, used.code);
            if !used.is_success() && is_no_makeup_needed(&message) {
                // “无需补登”不消耗卡，继续看下一个候选日期；同时标记本轮 streak 响应已过时。
                streak_stale = true;
                acc.parts.push(format!("{} 无需补登", display_value(date)));
                continue;
            }

            if used.is_success() {
                cards -= 1;
                cards_used += 1;
                streak_stale = true;

                let remaining = parse_makeup_cards(dig(&used.body, "makeup_cards"));
                let remaining = if dig(&used.body, "makeup_cards").is_some() {
                    remaining
                } else {
                    cards
                };

                acc.parts.push(format!(
                    "补登 {}（剩 {remaining} 张卡）",
                    display_value(date)
                ));
                acc.successes += 1;
            } else {
                acc.record_failure(
                    used.code,
                    format!("补登 {} 失败：{message}", display_value(date)),
                );
                // 真实失败后停止继续写，避免一次异常在同轮触发多个不可逆请求。
                break;
            }
        }

        let remaining_dates = dates.len().saturating_sub(dates_checked);
        if remaining_dates > 0 && cards > 0 {
            acc.parts.push(format!(
                "另有 {remaining_dates} 天可补、剩 {cards} 张卡，下轮继续"
            ));
        }
    }

    Ok(MakeupState {
        streak_body,
        streak_stale,
    })
}

fn is_no_makeup_needed(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("date is not broken")
        || lower.contains("no makeup needed")
        || message.contains("无需补登")
        || message.contains("不需要补登")
}

#[cfg(test)]
mod tests {
    use super::is_no_makeup_needed;

    #[test]
    fn no_makeup_needed_is_not_a_failure() {
        assert!(is_no_makeup_needed("date is not broken, no makeup needed"));
        assert!(is_no_makeup_needed("该日期无需补登"));
        assert!(!is_no_makeup_needed("insufficient makeup cards"));
    }
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
    async fn consumes_at_most_one_makeup_card_per_run() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/activity/growth/streak"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "makeup_cards": {"balance": 2},
                    "streak": {
                        "days": 10,
                        "makeup_dates": ["2026-09-01", "2026-09-02"]
                    }
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/activity/growth/makeup-cards/use"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data":{"makeup_cards":{"balance":1}}})),
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

        let state = run(&ctx, &mut acc).await.unwrap();
        assert!(state.streak_stale);
        assert_eq!(acc.successes, 1);
        assert!(acc.parts.iter().any(|part| part.contains("另有 1 天可补")));
    }
}
