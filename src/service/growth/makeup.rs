use serde_json::Value;

use crate::config::MAKEUP_MAX_PER_RUN;
use crate::model::common::ServiceRun;
use crate::service::signin::display_value;
use crate::util::{as_i64, client_token, dig};

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
        acc.parts.push("时间预算耗尽，补登跳过".to_string());
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
    let mut cards = parse_makeup_cards(dig(&response.body, "makeup_cards"));

    let nested_dates = dig(&response.body, "streak")
        .and_then(Value::as_object)
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
        let count = (cards as usize).min(MAKEUP_MAX_PER_RUN).min(dates.len());

        for date in dates.iter().take(count) {
            if ctx.budget.exhausted() {
                acc.parts.push("时间预算耗尽，剩余补登下次再做".to_string());
                break;
            }

            let used = ctx
                .api
                .use_makeup_card(date.clone(), client_token("u"))
                .await;

            if check_auth(used.code) {
                return Err(no_session());
            }

            if used.is_success() {
                cards -= 1;
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
                    format!(
                        "补登 {} 失败：{}",
                        display_value(date),
                        message_or_http(&used.body, used.code)
                    ),
                );
            }
        }

        if dates.len() > MAKEUP_MAX_PER_RUN && cards > 0 {
            acc.parts.push(format!(
                "另有 {} 天可补、剩 {} 张卡，下轮继续",
                dates.len() - MAKEUP_MAX_PER_RUN,
                cards
            ));
        }
    }

    Ok(MakeupState {
        streak_body,
        streak_stale,
    })
}
