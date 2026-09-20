use std::collections::HashMap;

use serde_json::{json, Value};

use crate::model::common::ServiceRun;
use crate::util::{dig, first_i64, value_truthy};

use super::context::{check_auth, message_or_http, no_session, GrowthAccumulator, GrowthContext};

pub async fn run(ctx: &GrowthContext<'_>, acc: &mut GrowthAccumulator) -> Option<ServiceRun> {
    if ctx.budget.exhausted() {
        acc.record_budget_exhausted("时间预算耗尽，任务领奖跳过");
        return None;
    }

    let response = ctx.api.tasks().await;

    if check_auth(response.code) {
        return Some(no_session());
    }

    if acc.note_http(&response, "查任务列表") {
        return None;
    }

    let Some(tasks_array) = dig(&response.body, "tasks").and_then(Value::as_array) else {
        acc.record_schema_mismatch("查任务列表", "缺少数组字段 tasks");
        return None;
    };
    let tasks: Vec<Value> = tasks_array.clone();

    if tasks.iter().any(|task| {
        !task.is_object() || task.get("task_code").and_then(Value::as_str).is_none()
    }) {
        acc.record_schema_mismatch("查任务列表", "tasks 中存在缺少 task_code 的条目");
    }

    let mut titles = HashMap::new();
    for task in &tasks {
        if let Some(code) = task.get("task_code").and_then(Value::as_str) {
            let title = task.get("title").and_then(Value::as_str).unwrap_or(code);
            titles.insert(code.to_string(), title.to_string());
        }
    }

    let pending: Vec<String> = tasks
        .iter()
        .filter_map(|task| {
            let code = task.get("task_code")?.as_str()?;
            let locked = value_truthy(task.get("locked"));
            let status = task.get("accept_status").and_then(Value::as_str);
            (!locked && status == Some("not_accepted")).then(|| code.to_string())
        })
        .collect();

    for batch in pending.chunks(20) {
        if ctx.budget.exhausted() {
            acc.record_budget_exhausted("时间预算耗尽，剩余任务下次再接单");
            break;
        }

        let accepted = ctx.api.accept_tasks(batch).await;

        if check_auth(accepted.code) {
            return Some(no_session());
        }

        let raw_results = dig(&accepted.body, "results");
        if raw_results.is_some() && raw_results.and_then(Value::as_array).is_none() {
            acc.record_schema_mismatch("接取任务", "results 存在但不是数组，使用批次结果回落");
        }
        let results: Vec<Value> = raw_results
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| {
                let status = if accepted.is_success() { "ok" } else { "error" };
                let message = dig(&accepted.body, "msg")
                    .cloned()
                    .unwrap_or(Value::Null);

                batch
                    .iter()
                    .map(|code| {
                        json!({
                            "task_code": code,
                            "status": status,
                            "message": message.clone()
                        })
                    })
                    .collect()
            });

        for result in results {
            let Some(code) = result.get("task_code").and_then(Value::as_str) else {
                acc.record_schema_mismatch("接取任务", "results 条目缺少 task_code");
                continue;
            };
            let title = titles.get(code).map(String::as_str).unwrap_or(code);
            let Some(result_status) = result.get("status").and_then(Value::as_str) else {
                acc.record_schema_mismatch("接取任务", "results 条目缺少 status");
                continue;
            };

            if result_status == "error" {
                let message = result
                    .get("message")
                    .filter(|value| !value.is_null())
                    .map(crate::service::signin::display_value)
                    .unwrap_or_else(|| format!("HTTP {}", accepted.code));

                acc.record_failure(accepted.code, format!("接取任务「{title}」失败：{message}"));
            } else {
                acc.parts.push(format!("已接取任务「{title}」"));
                acc.successes += 1;
            }
        }
    }

    for task in tasks {
        if ctx.budget.exhausted() {
            acc.record_budget_exhausted("时间预算耗尽，剩余任务奖下次再领");
            break;
        }

        if value_truthy(task.get("locked"))
            || task.get("accept_status").and_then(Value::as_str) != Some("completed")
        {
            continue;
        }

        let Some(code) = task.get("task_code").and_then(Value::as_str) else {
            continue;
        };
        let title = titles.get(code).map(String::as_str).unwrap_or(code);
        let claim = ctx.api.claim_task(code).await;

        if check_auth(claim.code) {
            return Some(no_session());
        }

        if claim.is_success() && !value_truthy(dig(&claim.body, "already_claimed")) {
            let credit = first_i64(&claim.body, "credit", task.get("reward_credit"));
            let energy = first_i64(&claim.body, "energy", task.get("reward_energy"));

            acc.credits_gained += credit;
            acc.parts
                .push(format!("领任务奖「{title}」+credit{credit}+energy{energy}"));
            acc.successes += 1;
        } else if claim.is_success() {
            acc.parts.push(format!("任务奖「{title}」已领过"));
        } else {
            acc.record_failure(
                claim.code,
                format!(
                    "领任务奖「{title}」失败：{}",
                    message_or_http(&claim.body, claim.code)
                ),
            );
        }
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
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::api::GrowthApi;
    use crate::budget::Budget;
    use crate::http::WorkBuddyClient;

    use super::*;
    use crate::service::growth::context::{GrowthAccumulator, GrowthContext};

    #[tokio::test]
    async fn accepts_tasks_in_batches_of_twenty_and_synthesizes_missing_results() {
        let server = MockServer::start().await;
        let codes: Vec<String> = (0..21).map(|index| format!("task-{index:02}")).collect();
        let tasks: Vec<_> = codes
            .iter()
            .map(|code| {
                json!({
                    "task_code": code,
                    "title": code,
                    "locked": false,
                    "accept_status": "not_accepted"
                })
            })
            .collect();

        Mock::given(method("GET"))
            .and(path("/v2/activity/growth/tasks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"tasks":tasks}})))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v2/activity/growth/tasks/accept"))
            .and(body_json(json!({"task_codes": &codes[..20]})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v2/activity/growth/tasks/accept"))
            .and(body_json(json!({"task_codes": &codes[20..]})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
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
        assert_eq!(acc.successes, 21);
        assert_eq!(
            acc.parts
                .iter()
                .filter(|part| part.starts_with("已接取任务"))
                .count(),
            21
        );
    }
}
