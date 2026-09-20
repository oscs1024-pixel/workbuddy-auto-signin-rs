use std::sync::Arc;

use reqwest::header::HeaderMap;
use serde_json::json;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use workbuddy_auto_signin::api::GrowthApi;
use workbuddy_auto_signin::budget::Budget;
use workbuddy_auto_signin::cli::Action;
use workbuddy_auto_signin::http::WorkBuddyClient;
use workbuddy_auto_signin::service::GrowthService;

async fn mock_get(server: &MockServer, route: &'static str, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn idle_growth_flow_runs_all_read_stages_and_reuses_streak() {
    let server = MockServer::start().await;

    mock_get(
        &server,
        "/v2/activity/growth/buddy/travel/status",
        json!({"data": {"state": "idle", "daily_limit_reached": true}}),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/tasks",
        json!({"data": {"tasks": []}}),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/streak",
        json!({
            "data": {
                "makeup_cards": {"balance": 0},
                "streak": {"days": 7, "makeup_dates": []}
            }
        }),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/redeem/summary",
        json!({
            "data": {
                "starter_status": "claimed",
                "advanced_status": "locked",
                "legendary_status": "locked"
            }
        }),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/lottery/chances",
        json!({"data": {"balance": 0}}),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/buddy/quota",
        json!({"data": {"affordable": 0, "max_open_count": 1}}),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/energy",
        json!({"data": {"balance": 3}}),
    )
    .await;

    let budget = Arc::new(Budget::for_action(Some(Action::Growth)).0);
    let client = WorkBuddyClient::new(
        Url::parse(&server.uri()).unwrap(),
        HeaderMap::new(),
        budget.clone(),
    )
    .unwrap();

    let run = GrowthService::new(GrowthApi::new(client), budget)
        .run()
        .await;

    assert_eq!(run.code, 0);
    assert_eq!(run.out["result"], "GROWTH");
    assert_eq!(run.out["idle"], true);
    assert_eq!(run.out["energy"], 3);
    assert_eq!(run.out["streak_days"], 7);
    assert!(run.out["report"]
        .as_str()
        .unwrap()
        .contains("今日旅行名额已用完"));
}


#[tokio::test]
async fn stale_makeup_dates_do_not_block_later_candidates() {
    let server = MockServer::start().await;

    mock_get(
        &server,
        "/v2/activity/growth/buddy/travel/status",
        json!({"data": {"state": "idle", "daily_limit_reached": true}}),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/tasks",
        json!({"data": {"tasks": []}}),
    )
    .await;

    // “无需补登”会让本轮 streak 快照过时，因此最终汇总会再查一次 streak。
    Mock::given(method("GET"))
        .and(path("/v2/activity/growth/streak"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "makeup_cards": {"balance": 1},
                "streak": {
                    "days": 20,
                    "makeup_dates": ["2026-09-06", "2026-09-07"]
                }
            }
        })))
        .expect(2)
        .mount(&server)
        .await;

    // 两个候选日期都返回“无需补登”。关键断言是必须发出两次请求：
    // 第一条陈旧日期不能占用“每轮最多消耗一张卡”的额度并挡住第二条。
    Mock::given(method("POST"))
        .and(path("/v2/activity/growth/makeup-cards/use"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "msg": "date is not broken, no makeup needed"
        })))
        .expect(2)
        .mount(&server)
        .await;

    mock_get(
        &server,
        "/v2/activity/growth/redeem/summary",
        json!({
            "data": {
                "starter_status": "claimed",
                "advanced_status": "locked",
                "legendary_status": "locked"
            }
        }),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/lottery/chances",
        json!({"data": {"balance": 0}}),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/buddy/quota",
        json!({"data": {"affordable": 0, "max_open_count": 1}}),
    )
    .await;
    mock_get(
        &server,
        "/v2/activity/growth/energy",
        json!({"data": {"balance": 0}}),
    )
    .await;

    let budget = Arc::new(Budget::for_action(Some(Action::Growth)).0);
    let client = WorkBuddyClient::new(
        Url::parse(&server.uri()).unwrap(),
        HeaderMap::new(),
        budget.clone(),
    )
    .unwrap();

    let run = GrowthService::new(GrowthApi::new(client), budget)
        .run()
        .await;

    assert_eq!(run.code, 0);
    assert_eq!(run.out["result"], "GROWTH");
    assert_eq!(run.out["idle"], true);
    assert!(run.out.get("failures").is_none());
    let report = run.out["report"].as_str().unwrap();
    assert!(report.contains("2026-09-06 无需补登"));
    assert!(report.contains("2026-09-07 无需补登"));
}
