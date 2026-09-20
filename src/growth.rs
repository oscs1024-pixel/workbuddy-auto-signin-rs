use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::api::GrowthApi;
use crate::budget::Budget;
use crate::http::{http_label, is_hard_failure, HttpResult, CODE_BUDGET_OUT, CODE_NO_NETWORK};
use crate::signin::{display_value, ServiceRun};
use crate::util::{as_i64, client_token, dig, first_i64, format_eta, value_string, value_truthy};

pub const MAKEUP_MAX_PER_RUN: usize = 1;

const REDEEM_TIERS: &[(&str, &str, &str, i64)] = &[
    ("7d", "starter", "入门", 7),
    ("14d", "advanced", "进阶", 14),
    ("28d", "legendary", "巅峰", 28),
];

#[derive(Debug, Default)]
struct GrowthAccumulator {
    parts: Vec<String>,
    credits_gained: i64,
    failures: usize,
    hard_failures: usize,
    successes: usize,
}

impl GrowthAccumulator {
    fn note_http(&mut self, response: &HttpResult, label: &str) -> bool {
        if response.is_success() {
            return false;
        }
        let reason = http_label(response.code);
        let detail = response.body.as_object()
            .and_then(|o| o.get("error").or_else(|| o.get("msg")))
            .map(display_value)
            .unwrap_or_default();
        let description = if !detail.is_empty() && detail != reason {
            format!("{reason}（{detail}）")
        } else if !detail.is_empty() {
            detail
        } else {
            reason
        };
        self.parts.push(format!("{label}失败：{description}"));
        if is_hard_failure(response.code) {
            self.failures += 1;
            self.hard_failures += 1;
        }
        true
    }

    fn record_failure(&mut self, code: i32, message: String) {
        self.parts.push(message);
        self.failures += 1;
        if is_hard_failure(code) {
            self.hard_failures += 1;
        }
    }
}

#[derive(Clone)]
pub struct GrowthService {
    api: GrowthApi,
    budget: Arc<Budget>,
}

impl GrowthService {
    pub fn new(api: GrowthApi, budget: Arc<Budget>) -> Self {
        Self { api, budget }
    }

    pub async fn run(&self) -> ServiceRun {
        let mut acc = GrowthAccumulator::default();

        if let Some(run) = self.run_travel(&mut acc).await {
            return run;
        }
        if let Some(run) = self.run_tasks(&mut acc).await {
            return run;
        }
        let (streak_body, streak_stale, early) = self.run_makeup(&mut acc).await;
        if let Some(run) = early {
            return run;
        }
        if let Some(run) = self.run_redeem(&mut acc).await {
            return run;
        }
        if let Some(run) = self.run_lottery(&mut acc).await {
            return run;
        }
        if let Some(run) = self.run_buddy_box(&mut acc).await {
            return run;
        }

        let (energy, streak_days) = self.load_summary_values(streak_body, streak_stale).await;
        finalize(acc, energy, streak_days)
    }

    async fn run_travel(&self, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
        let status = self.api.travel_status().await;
        if status.code == CODE_BUDGET_OUT {
            return Some(ServiceRun { code: 1, out: json!({
                "result":"TIMEOUT",
                "report":"时间预算耗尽，成长中心跳过，下次自动重试"
            })});
        }
        if status.code == CODE_NO_NETWORK {
            let error = dig(&status.body, "error").and_then(Value::as_str).unwrap_or("");
            return Some(ServiceRun { code: 1, out: json!({
                "result":"NETWORK",
                "report":format!("网络不可达，成长中心跳过（{error}）")
            })});
        }
        if check_auth(status.code) {
            return Some(no_session());
        }

        let mut travel = if status.is_success() {
            dig(&status.body, "state").and_then(Value::as_str).map(str::to_string)
        } else {
            None
        };
        let daily_limit = status.is_success() && value_truthy(dig(&status.body, "daily_limit_reached"));
        acc.note_http(&status, "查旅行状态");

        if travel.as_deref() == Some("arrived") {
            let record_id = dig(&status.body, "record_id").cloned().unwrap_or(Value::Null);
            let claim = self.api.travel_claim(record_id).await;
            if check_auth(claim.code) {
                return Some(no_session());
            }
            if claim.is_success() && dig(&claim.body, "reward_credit").is_some() {
                let got = as_i64(dig(&claim.body, "reward_credit"), 0);
                acc.credits_gained += got;
                acc.parts.push(format!("领旅行礼物 +{} 积分", got));
                acc.successes += 1;
                travel = Some("idle".to_string());
            } else {
                let msg = dig(&claim.body, "msg").and_then(Value::as_str).unwrap_or("");
                acc.record_failure(
                    claim.code,
                    format!("领旅行礼物失败：{}", if msg.is_empty() { format!("HTTP {}", claim.code) } else { msg.to_string() }),
                );
            }
        }

        if travel.as_deref() == Some("idle") && daily_limit {
            acc.parts.push("今日旅行名额已用完".to_string());
        } else if travel.as_deref() == Some("idle") {
            let config = self.api.travel_config().await;
            if check_auth(config.code) {
                return Some(no_session());
            }
            if let Some(location) = dig(&config.body, "locations")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_object)
            {
                let location_id = location.get("id").cloned().unwrap_or(Value::Null);
                let depart = self.api.travel_depart(location_id).await;
                if check_auth(depart.code) {
                    return Some(no_session());
                }
                if depart.is_success() {
                    let loc_name = dig(&depart.body, "location")
                        .and_then(Value::as_object)
                        .and_then(|o| o.get("name"))
                        .map(display_value)
                        .unwrap_or_else(|| "?".to_string());
                    let duration = dig(&depart.body, "duration_hours")
                        .cloned()
                        .or_else(|| dig(&depart.body, "location").and_then(Value::as_object).and_then(|o| o.get("duration_hours")).cloned())
                        .map(|v| display_value(&v))
                        .unwrap_or_else(|| "?".to_string());
                    acc.parts.push(format!("派 Buddy 去{loc_name}（{duration} 小时后回）"));
                    acc.successes += 1;
                } else {
                    let msg = dig(&depart.body, "msg").and_then(Value::as_str).unwrap_or("");
                    acc.record_failure(
                        depart.code,
                        format!("派 Buddy 失败：{}", if msg.is_empty() { format!("HTTP {}", depart.code) } else { msg.to_string() }),
                    );
                }
            }
        } else if travel.as_deref() == Some("traveling") {
            let loc_name = dig(&status.body, "location")
                .and_then(Value::as_object)
                .and_then(|o| o.get("name"))
                .map(display_value)
                .unwrap_or_else(|| "?".to_string());
            let eta = format_eta(dig(&status.body, "arrive_at"), dig(&status.body, "server_now"));
            acc.parts.push(format!("Buddy 旅行中（{loc_name}{eta}）"));
        }

        None
    }

    async fn run_tasks(&self, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
        if self.budget.exhausted() {
            acc.parts.push("时间预算耗尽，任务领奖跳过".to_string());
            return None;
        }

        let response = self.api.tasks().await;
        if check_auth(response.code) {
            return Some(no_session());
        }
        if acc.note_http(&response, "查任务列表") {
            return None;
        }

        let tasks: Vec<Value> = dig(&response.body, "tasks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut titles = HashMap::new();
        for task in &tasks {
            if let Some(code) = task.get("task_code").and_then(Value::as_str) {
                let title = task.get("title").and_then(Value::as_str).unwrap_or(code);
                titles.insert(code.to_string(), title.to_string());
            }
        }

        let pending: Vec<String> = tasks.iter().filter_map(|task| {
            let code = task.get("task_code")?.as_str()?;
            let locked = value_truthy(task.get("locked"));
            let status = task.get("accept_status").and_then(Value::as_str);
            (!locked && status == Some("not_accepted")).then(|| code.to_string())
        }).collect();

        for batch in pending.chunks(20) {
            if self.budget.exhausted() {
                acc.parts.push("时间预算耗尽，剩余任务下次再接单".to_string());
                break;
            }
            let accepted = self.api.accept_tasks(batch).await;
            if check_auth(accepted.code) {
                return Some(no_session());
            }
            let results: Vec<Value> = dig(&accepted.body, "results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| {
                    let status = if accepted.is_success() { "ok" } else { "error" };
                    let message = dig(&accepted.body, "msg").cloned().unwrap_or(Value::Null);
                    batch.iter().map(|code| json!({
                        "task_code": code,
                        "status": status,
                        "message": message.clone()
                    })).collect()
                });

            for result in results {
                let code = result.get("task_code").and_then(Value::as_str).unwrap_or("");
                let title = titles.get(code).map(String::as_str).unwrap_or(code);
                if result.get("status").and_then(Value::as_str) == Some("error") {
                    let message = result.get("message").filter(|v| !v.is_null()).map(display_value)
                        .unwrap_or_else(|| format!("HTTP {}", accepted.code));
                    acc.record_failure(accepted.code, format!("领取任务「{title}」失败：{message}"));
                } else {
                    acc.parts.push(format!("领取任务「{title}」（进度开始计）"));
                    acc.successes += 1;
                }
            }
        }

        for task in tasks {
            if self.budget.exhausted() {
                acc.parts.push("时间预算耗尽，剩余任务奖下次再领".to_string());
                break;
            }
            if value_truthy(task.get("locked")) || task.get("accept_status").and_then(Value::as_str) != Some("completed") {
                continue;
            }
            let Some(code) = task.get("task_code").and_then(Value::as_str) else { continue; };
            let title = titles.get(code).map(String::as_str).unwrap_or(code);
            let claim = self.api.claim_task(code).await;
            if check_auth(claim.code) {
                return Some(no_session());
            }
            if claim.is_success() && !value_truthy(dig(&claim.body, "already_claimed")) {
                let credit = first_i64(&claim.body, "credit", task.get("reward_credit"));
                let energy = first_i64(&claim.body, "energy", task.get("reward_energy"));
                acc.credits_gained += credit;
                acc.parts.push(format!("领任务奖「{title}」+credit{credit}+energy{energy}"));
                acc.successes += 1;
            } else if claim.is_success() {
                acc.parts.push(format!("任务奖「{title}」已领过"));
            } else {
                let msg = dig(&claim.body, "msg").and_then(Value::as_str).unwrap_or("");
                acc.record_failure(
                    claim.code,
                    format!("领任务奖「{title}」失败：{}", if msg.is_empty() { format!("HTTP {}", claim.code) } else { msg.to_string() }),
                );
            }
        }

        None
    }

    async fn run_makeup(&self, acc: &mut GrowthAccumulator) -> (Option<Value>, bool, Option<ServiceRun>) {
        if self.budget.exhausted() {
            acc.parts.push("时间预算耗尽，补登跳过".to_string());
            return (None, false, None);
        }

        let response = self.api.streak().await;
        if check_auth(response.code) {
            return (None, false, Some(no_session()));
        }
        if acc.note_http(&response, "查连登状态") {
            return (None, false, None);
        }

        let streak_body = Some(response.body.clone());
        let cards_value = dig(&response.body, "makeup_cards");
        let mut cards = match cards_value.and_then(Value::as_object) {
            Some(obj) => as_i64(obj.get("balance"), 0),
            None => as_i64(cards_value, 0),
        };

        let nested_dates = dig(&response.body, "streak")
            .and_then(Value::as_object)
            .and_then(|o| o.get("makeup_dates"))
            .and_then(Value::as_array);
        let dates: Vec<Value> = match nested_dates {
            Some(items) if !items.is_empty() => items.clone(),
            _ => dig(&response.body, "makeup_dates").and_then(Value::as_array).cloned().unwrap_or_default(),
        };

        let mut stale = false;
        if cards > 0 && !dates.is_empty() {
            let count = (cards as usize).min(MAKEUP_MAX_PER_RUN).min(dates.len());
            for date in dates.iter().take(count) {
                if self.budget.exhausted() {
                    acc.parts.push("时间预算耗尽，剩余补登下次再做".to_string());
                    break;
                }
                let used = self.api.use_makeup_card(date.clone(), client_token("u")).await;
                if check_auth(used.code) {
                    return (streak_body, stale, Some(no_session()));
                }
                if used.is_success() {
                    cards -= 1;
                    stale = true;
                    let remaining_value = dig(&used.body, "makeup_cards");
                    let remaining = match remaining_value.and_then(Value::as_object) {
                        Some(obj) => as_i64(obj.get("balance"), cards),
                        None => as_i64(remaining_value, cards),
                    };
                    acc.parts.push(format!("补登 {}（剩 {remaining} 张卡）", display_value(date)));
                    acc.successes += 1;
                } else {
                    let msg = dig(&used.body, "msg").and_then(Value::as_str).unwrap_or("");
                    acc.record_failure(
                        used.code,
                        format!("补登 {} 失败：{}", display_value(date), if msg.is_empty() { format!("HTTP {}", used.code) } else { msg.to_string() }),
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

        (streak_body, stale, None)
    }

    async fn run_redeem(&self, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
        if self.budget.exhausted() {
            acc.parts.push("时间预算耗尽，连登兑换跳过".to_string());
            return None;
        }

        let summary = self.api.redeem_summary().await;
        if check_auth(summary.code) {
            return Some(no_session());
        }
        if acc.note_http(&summary, "查连登兑换") {
            return None;
        }

        for (tier, status_key, label, days) in REDEEM_TIERS {
            if self.budget.exhausted() {
                acc.parts.push("时间预算耗尽，剩余连登兑换下次再领".to_string());
                break;
            }
            let key = format!("{status_key}_status");
            let Some(status) = dig(&summary.body, &key).and_then(Value::as_str) else { continue; };
            if status.is_empty() || matches!(status, "claimed" | "locked") {
                continue;
            }

            let mut redeemed = self.api.redeem(json!(tier), client_token("u")).await;
            if is_unknown_tier(&redeemed) {
                redeemed = self.api.redeem(json!(days), client_token("u")).await;
            }
            if is_tier_locked(&redeemed) {
                acc.parts.push(format!("连登兑换「{label}」未解锁（连登天数不足）"));
                continue;
            }
            if check_auth(redeemed.code) {
                return Some(no_session());
            }
            if redeemed.is_success() {
                acc.credits_gained += as_i64(dig(&redeemed.body, "credit_granted"), 0);
                acc.parts.push(format!("连登兑换「{label}」{}", redeem_reward_desc(&redeemed.body, tier)));
                acc.successes += 1;
            } else {
                let msg = dig(&redeemed.body, "msg").and_then(Value::as_str).unwrap_or("");
                acc.record_failure(
                    redeemed.code,
                    format!("连登兑换「{label}」失败：{}", if msg.is_empty() { format!("HTTP {}", redeemed.code) } else { msg.to_string() }),
                );
            }
        }
        None
    }

    async fn run_lottery(&self, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
        if self.budget.exhausted() {
            acc.parts.push("时间预算耗尽，盲盒跳过".to_string());
            return None;
        }
        let chances_response = self.api.lottery_chances().await;
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

        let draw = self.api.lottery_draw(client_token("u")).await;
        if check_auth(draw.code) {
            return Some(no_session());
        }
        if draw.is_success() {
            let prize_value = dig(&draw.body, "prize_name")
                .or_else(|| dig(&draw.body, "prize"));
            let mut prize = match prize_value {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => "未知".to_string(),
            };
            if value_truthy(dig(&draw.body, "need_address")) || value_truthy(dig(&draw.body, "require_address")) {
                prize.push_str("（实物奖，需到成长中心填写收件信息）");
            }
            acc.parts.push(format!("开盲盒获得：{prize}"));
            acc.successes += 1;
            if chances > 1 {
                acc.parts.push(format!("还剩 {} 次抽奖机会，下轮继续", chances - 1));
            }
        } else {
            let msg = dig(&draw.body, "msg").and_then(Value::as_str).unwrap_or("");
            if is_no_chance(msg) {
                acc.parts.push(format!("开盲盒：{}", if msg.is_empty() { "无抽奖机会" } else { msg }));
            } else {
                acc.record_failure(
                    draw.code,
                    format!("开盲盒失败：{}", if msg.is_empty() { format!("HTTP {}", draw.code) } else { msg.to_string() }),
                );
            }
        }
        None
    }

    async fn run_buddy_box(&self, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
        if self.budget.exhausted() {
            acc.parts.push("时间预算耗尽，Buddy 盲盒跳过".to_string());
            return None;
        }
        let quota = self.api.buddy_quota().await;
        if check_auth(quota.code) {
            return Some(no_session());
        }
        if acc.note_http(&quota, "查 Buddy 能量") {
            return None;
        }

        let affordable = as_i64(dig(&quota.body, "affordable"), 0);
        let mut max_open = as_i64(dig(&quota.body, "max_open_count"), 1);
        if max_open == 0 { max_open = 1; }
        if affordable <= 0 {
            return None;
        }
        let count = affordable.min(max_open);
        let opened = self.api.buddy_open(count, client_token("u")).await;
        if check_auth(opened.code) {
            return Some(no_session());
        }
        if opened.is_success() {
            let name = ["buddy", "name", "buddies"].iter()
                .find_map(|key| dig(&opened.body, key).and_then(Value::as_str))
                .unwrap_or("新 Buddy");
            acc.parts.push(format!("开 Buddy 盲盒 ×{count}（{name}）"));
            acc.successes += 1;
        } else {
            let msg = dig(&opened.body, "msg").and_then(Value::as_str).unwrap_or("");
            acc.record_failure(
                opened.code,
                format!("开 Buddy 盲盒失败：{}", if msg.is_empty() { format!("HTTP {}", opened.code) } else { msg.to_string() }),
            );
        }
        None
    }

    async fn load_summary_values(&self, streak_body: Option<Value>, streak_stale: bool) -> (Option<Value>, Option<Value>) {
        let mut energy = None;
        if !self.budget.exhausted() {
            let response = self.api.energy().await;
            if !check_auth(response.code) && response.is_success() {
                energy = dig(&response.body, "balance").cloned();
            }
        }

        let streak_days = if let Some(body) = streak_body.filter(|_| !streak_stale) {
            dig(&body, "streak")
                .and_then(Value::as_object)
                .and_then(|o| o.get("days"))
                .cloned()
        } else if !self.budget.exhausted() {
            let response = self.api.streak().await;
            if !check_auth(response.code) {
                dig(&response.body, "streak")
                    .and_then(Value::as_object)
                    .and_then(|o| o.get("days"))
                    .cloned()
            } else {
                None
            }
        } else {
            None
        };

        (energy, streak_days)
    }
}

fn finalize(acc: GrowthAccumulator, energy: Option<Value>, streak_days: Option<Value>) -> ServiceRun {
    let mut tail = Vec::new();
    if let Some(value) = energy.as_ref() {
        tail.push(format!("能量 {}", display_value(value)));
    }
    if let Some(value) = streak_days.as_ref() {
        tail.push(format!("连签 {} 天", display_value(value)));
    }
    if acc.credits_gained != 0 {
        tail.push(format!("本次 +共 {} 积分", acc.credits_gained));
    }

    let mut report = if !acc.parts.is_empty() {
        acc.parts.join("；")
    } else if acc.failures > 0 {
        "成长中心各步骤均失败".to_string()
    } else {
        "成长中心无可领取项".to_string()
    };
    if !tail.is_empty() {
        report.push_str(&format!("（{}）", tail.join("，")));
    }

    let code = if acc.hard_failures > 0 && acc.successes == 0 { 1 } else { 0 };
    let idle = acc.successes == 0 && acc.failures == 0;
    let mut out = Map::new();
    out.insert("result".into(), json!("GROWTH"));
    out.insert("report".into(), json!(report));
    out.insert("credits_gained".into(), json!(acc.credits_gained));
    out.insert("energy".into(), energy.unwrap_or(Value::Null));
    out.insert("streak_days".into(), streak_days.unwrap_or(Value::Null));
    out.insert("idle".into(), json!(idle));
    if acc.failures > 0 {
        out.insert("failures".into(), json!(acc.failures));
    }
    ServiceRun { code, out: Value::Object(out) }
}

fn no_session() -> ServiceRun {
    ServiceRun { code: 1, out: json!({
        "result":"NO_SESSION",
        "report":"登录态已失效，请重新登录 WorkBuddy 桌面端"
    })}
}

fn check_auth(code: i32) -> bool {
    matches!(code, 401 | 403)
}

pub fn is_no_chance(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    if m.is_empty() {
        return false;
    }
    if m.contains("insufficient") || m.contains("not enough") {
        return m.contains("chance") || m.contains("balance");
    }
    m.contains("no chance")
}

pub fn is_unknown_tier(response: &HttpResult) -> bool {
    if response.code != 400 {
        return false;
    }
    let message = value_string(dig(&response.body, "msg")).unwrap_or_default().to_ascii_lowercase();
    message.contains("tier") && (message.contains("unknown") || message.contains("unsupported") || message.contains("invalid"))
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
    if credit != 0 { bits.push(format!("+{} 积分", credit)); }
    if energy != 0 { bits.push(format!("+{} 能量", energy)); }
    if cards != 0 { bits.push(format!("+{} 补登卡", cards)); }
    if chances != 0 { bits.push(format!("+{} 次抽奖", chances)); }
    if !bits.is_empty() {
        return format!("（{}）", bits.join(" "));
    }
    let fallback = match tier {
        "7d" => "+2 能量 +1 补登卡 +1 次抽奖",
        "14d" => "+50 积分 +3 能量 +1 补登卡 +1 次抽奖",
        "28d" => "+150 积分 +5 能量 +1 补登卡 +1 次抽奖",
        _ => "奖励已到账",
    };
    format!("（{fallback}）")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_chance_business_error_is_normal() {
        assert!(is_no_chance("insufficient lottery chance balance"));
        assert!(is_no_chance("not enough chance"));
        assert!(!is_no_chance("invalid request"));
    }

    #[test]
    fn redeem_unknown_tier_requires_tier_word() {
        assert!(is_unknown_tier(&HttpResult { code: 400, body: json!({"msg":"unknown tier"}) }));
        assert!(!is_unknown_tier(&HttpResult { code: 400, body: json!({"msg":"invalid request"}) }));
    }

    #[test]
    fn tier_locked_403_is_distinct_from_auth_failure() {
        assert!(is_tier_locked(&HttpResult { code: 403, body: json!({"msg":"连续登录天数不足"}) }));
        assert!(!is_tier_locked(&HttpResult { code: 403, body: json!({"msg":"unauthorized"}) }));
    }

    #[test]
    fn redeem_reward_uses_granted_fields() {
        let desc = redeem_reward_desc(&json!({
            "credit_granted": 50,
            "energy_granted": 3,
            "cards_granted": 1,
            "chances_granted": 1
        }), "14d");
        assert_eq!(desc, "（+50 积分 +3 能量 +1 补登卡 +1 次抽奖）");
    }

    #[test]
    fn redeem_reward_has_tier_fallback() {
        assert_eq!(redeem_reward_desc(&json!({}), "7d"), "（+2 能量 +1 补登卡 +1 次抽奖）");
    }
}
