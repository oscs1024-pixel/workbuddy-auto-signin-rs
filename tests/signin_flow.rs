use std::sync::Arc;

use reqwest::header::HeaderMap;
use serde_json::json;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use workbuddy_auto_signin::api::BillingApi;
use workbuddy_auto_signin::budget::Budget;
use workbuddy_auto_signin::http::WorkBuddyClient;
use workbuddy_auto_signin::signin::SigninService;

#[tokio::test]
async fn unchecked_day_is_claimed_and_reported() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/billing/meter/checkin-activity-status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "active": true,
            "today_checked_in": false,
            "streak_days": 7,
            "total_credits": 700
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/billing/meter/daily-checkin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"credit":100})))
        .mount(&server)
        .await;

    let budget = Arc::new(Budget::for_action(None).0);
    let client = WorkBuddyClient::new(Url::parse(&server.uri()).unwrap(), HeaderMap::new(), budget).unwrap();
    let service = SigninService::new(BillingApi::new(client));
    let run = service.run().await;

    assert_eq!(run.code, 0);
    assert_eq!(run.out["result"], "CLAIMED");
    assert_eq!(run.out["credit"], 100);
    assert!(run.out["report"].as_str().unwrap().contains("成功领取 100 积分"));
}

#[tokio::test]
async fn null_claim_response_is_treated_as_already_checked_in() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/billing/meter/checkin-activity-status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "active": true,
            "today_checked_in": false,
            "today_credit": 100
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/billing/meter/daily-checkin"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("null", "application/json"))
        .mount(&server)
        .await;

    let budget = Arc::new(Budget::for_action(None).0);
    let client = WorkBuddyClient::new(Url::parse(&server.uri()).unwrap(), HeaderMap::new(), budget).unwrap();
    let service = SigninService::new(BillingApi::new(client));
    let run = service.run().await;

    assert_eq!(run.code, 0);
    assert_eq!(run.out["result"], "ALREADY");
}
