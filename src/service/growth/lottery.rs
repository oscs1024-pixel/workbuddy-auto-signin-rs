use serde_json::Value;

use crate::model::common::ServiceRun;
use crate::util::{as_i64, client_token, dig, value_truthy};

use super::context::{
    check_auth, message_or_http, no_session, GrowthAccumulator, GrowthContext,
};

pub async fn run(
    ctx: &GrowthContext<'_>,
    acc: &mut GrowthAccumulator,
) -> Option<ServiceRun> {
    if ctx.budget.exhausted() {
        acc.parts.push("时间预算耗尽，盲盒跳过".to_string());
        return None;
    }

    let chances_response = ctx.api.lottery_chances().await;

    if check_auth(chances_response.code) {
        return Some(no_session());
    }

    let chances = if acc.note_http(&chances_response, "查抽奖机会") {
        0
    } else {
        as_i64(dig(&chances_response.body, "balance"), 0)
    };

    if chances <= 0 {
        return None;
    }

    let draw = ctx.api.lottery_draw(client_token("u")).await;

    if check_auth(draw.code) {
        return Some(no_session());
    }

    if draw.is_success() {
        let prize_value = dig(&draw.body, "prize_name")
            .or_else(|| dig(&draw.body, "prize"));

        let mut prize = match prize_value {
            Some(Value::String(value)) => value.clone(),
            Some(other) => other.to_string(),
            None => "未知".to_string(),
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
        let message = dig(&draw.body, "msg")
            .and_then(Value::as_str)
            .unwrap_or("");

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
                format!(
                    "开盲盒失败：{}",
                    message_or_http(&draw.body, draw.code)
                ),
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

    if message.contains("insufficient")
        || message.contains("not enough")
    {
        return message.contains("chance") || message.contains("balance");
    }

    message.contains("no chance")
}
