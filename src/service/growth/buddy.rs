use serde_json::Value;

use crate::model::common::ServiceRun;
use crate::util::{client_token, dig, try_i64};

use super::context::{check_auth, message_or_http, no_session, GrowthAccumulator, GrowthContext};

pub fn compute_open_count(affordable: i64, max_open_count: i64) -> i64 {
    affordable.min(max_open_count.max(1))
}

pub async fn run(ctx: &GrowthContext<'_>, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
    if ctx.budget.exhausted() {
        acc.record_budget_exhausted("时间预算耗尽，Buddy 盲盒跳过");
        return None;
    }

    let quota = ctx.api.buddy_quota().await;

    if check_auth(quota.code) {
        return Some(no_session());
    }

    if acc.note_http(&quota, "查 Buddy 能量") {
        return None;
    }

    let Some(affordable) = try_i64(dig(&quota.body, "affordable")) else {
        acc.record_schema_mismatch("查 Buddy 能量", "缺少或无法解析 affordable");
        return None;
    };
    let max_open_count = match dig(&quota.body, "max_open_count") {
        Some(value) => match try_i64(Some(value)) {
            Some(value) => value,
            None => {
                acc.record_schema_mismatch("查 Buddy 能量", "max_open_count 无法解析");
                1
            }
        },
        None => 1,
    };

    if affordable <= 0 {
        return None;
    }

    let count = compute_open_count(affordable, max_open_count);
    let opened = ctx.api.buddy_open(count, client_token("u")).await;

    if check_auth(opened.code) {
        return Some(no_session());
    }

    if opened.is_success() {
        let name = ["buddy", "name", "buddies"]
            .iter()
            .find_map(|key| dig(&opened.body, key).and_then(Value::as_str))
            .unwrap_or("新 Buddy");

        acc.parts.push(format!("开 Buddy 盲盒 ×{count}（{name}）"));
        acc.successes += 1;
    } else {
        acc.record_failure(
            opened.code,
            format!(
                "开 Buddy 盲盒失败：{}",
                message_or_http(&opened.body, opened.code)
            ),
        );
    }

    None
}
