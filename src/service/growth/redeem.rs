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
        acc.parts.push("时间预算耗尽，连登兑换跳过".to_string());
        return None;
    }

    let summary = ctx.api.redeem_summary().await;

    if check_auth(summary.code) {
        return Some(no_session());
    }

    if acc.note_http(&summary, "查连登兑换") {
        return None;
    }

    for (tier, status_key, label, days) in REDEEM_TIERS {
        if ctx.budget.exhausted() {
            acc.parts
                .push("时间预算耗尽，剩余连登兑换下次再领".to_string());
            break;
        }

        let key = format!("{status_key}_status");
        let Some(status) = dig(&summary.body, &key).and_then(Value::as_str) else {
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

    message.contains("天数不足") || message.contains("不足")
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
