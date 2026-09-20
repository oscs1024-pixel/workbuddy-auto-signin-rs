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
async fn task_accept_uses_plural_task_codes_contract() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/activity/growth/tasks/accept"))
        .and(body_json(json!({"task_codes":["a","b"]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results":[]})))
        .expect(1)
        .mount(&server)
        .await;

    let client = WorkBuddyClient::new(
        Url::parse(&server.uri()).unwrap(),
        HeaderMap::new(),
        Arc::new(Budget::for_action(None).0),
    ).unwrap();
    let api = GrowthApi::new(client);
    let result = api.accept_tasks(&["a".into(), "b".into()]).await;
    assert_eq!(result.code, 200);
}

#[tokio::test]
async fn redeem_accepts_string_tier_and_client_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/activity/growth/redeem"))
        .and(body_json(json!({"tier":"14d","client_token":"u-test"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"credit_granted":50})))
        .expect(1)
        .mount(&server)
        .await;

    let client = WorkBuddyClient::new(
        Url::parse(&server.uri()).unwrap(),
        HeaderMap::new(),
        Arc::new(Budget::for_action(None).0),
    ).unwrap();
    let api = GrowthApi::new(client);
    let result = api.redeem(json!("14d"), "u-test".into()).await;
    assert_eq!(result.code, 200);
}
