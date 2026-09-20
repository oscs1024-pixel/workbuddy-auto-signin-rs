use std::sync::Arc;

use reqwest::header::HeaderMap;
use serde_json::json;
use url::Url;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use workbuddy_auto_signin::api::GrowthApi;
use workbuddy_auto_signin::budget::Budget;
use workbuddy_auto_signin::http::WorkBuddyClient;

#[tokio::test]
async fn accept_tasks_uses_plural_task_codes() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/activity/growth/tasks/accept"))
        .and(body_json(json!({
            "task_codes": ["a", "b"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": []})))
        .expect(1)
        .mount(&server)
        .await;

    let client = WorkBuddyClient::new(
        Url::parse(&server.uri()).unwrap(),
        HeaderMap::new(),
        Arc::new(Budget::for_action(None).0),
    )
    .unwrap();

    let response = GrowthApi::new(client)
        .accept_tasks(&["a".into(), "b".into()])
        .await;

    assert_eq!(response.code, 200);
}
