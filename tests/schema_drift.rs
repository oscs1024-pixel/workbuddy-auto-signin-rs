use std::sync::Arc;
use std::time::Duration;

use reqwest::header::HeaderMap;
use serde_json::json;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use workbuddy_auto_signin::api::GrowthApi;
use workbuddy_auto_signin::budget::Budget;
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
async fn successful_http_with_missing_required_schema_is_not_idle() {
    let server = MockServer::start().await;

    mock_get(
        &server,
        "/v2/activity/growth/buddy/travel/status",
        json!({
            "data": {
                "state": "traveling",
                "location": {"name": "咖啡馆"},
                "arrive_at": 10256,
                "server_now": 0
            }
        }),
    )
    .await;

    // HTTP 200 但 tasks 字段消失：必须作为 schema mismatch 暴露，而不是当空任务。
    mock_get(&server, "/v2/activity/growth/tasks", json!({"data": {}})).await;
    mock_get(
        &server,
        "/v2/activity/growth/streak",
        json!({
            "data": {
                "makeup_cards": {"balance": 0},
                "streak": {"days": 20, "makeup_dates": []}
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
        json!({"data": {"balance": 0}}),
    )
    .await;

    let budget = Arc::new(Budget::with_limit(Duration::from_secs(5)));
    let client = WorkBuddyClient::new(
        Url::parse(&server.uri()).unwrap(),
        HeaderMap::new(),
        budget.clone(),
    )
    .unwrap();

    let run = GrowthService::new(GrowthApi::new(client), budget)
        .run()
        .await;

    // schema drift 是需要记录的 soft failure：不把调度任务报红，但必须阻止 silent idle。
    assert_eq!(run.code, 0);
    assert_eq!(run.out["idle"], false);
    assert_eq!(run.out["failures"], 1);
    assert_eq!(run.out["schema_mismatches"], 1);
    assert!(run.out["report"]
        .as_str()
        .unwrap()
        .contains("查任务列表响应结构异常"));
}
