use std::sync::Arc;

use reqwest::header::HeaderMap;
use serde_json::Value;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use workbuddy_auto_signin::api::{BillingApi, GrowthApi};
use workbuddy_auto_signin::budget::Budget;
use workbuddy_auto_signin::http::WorkBuddyClient;
use workbuddy_auto_signin::service::{GrowthService, SigninService};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/reference_differential.json")).unwrap()
}

fn assert_subset(actual: &Value, expected: &Value) {
    match expected {
        Value::Object(expected) => {
            let actual = actual.as_object().expect("actual value must be an object");
            for (key, value) in expected {
                let actual_value = actual
                    .get(key)
                    .unwrap_or_else(|| panic!("actual output missing key {key}"));
                assert_subset(actual_value, value);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

fn request_trace(requests: &[wiremock::Request]) -> Vec<Value> {
    requests
        .iter()
        .map(|request| {
            serde_json::json!({
                "method": request.method.to_string(),
                "path": request.url.path()
            })
        })
        .collect()
}

fn client(server: &MockServer, budget: Arc<Budget>) -> WorkBuddyClient {
    WorkBuddyClient::new(
        Url::parse(&server.uri()).unwrap(),
        HeaderMap::new(),
        budget,
    )
    .unwrap()
}

#[tokio::test]
async fn signin_request_and_result_match_pinned_reference_contract() {
    let fixture = fixture();
    let case = &fixture["signin_already"];
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/billing/meter/checkin-activity-status"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(case["status_response"].clone()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let budget = Arc::new(Budget::with_limit(std::time::Duration::from_secs(5)));
    let service = SigninService::new(BillingApi::new(client(&server, budget)));
    let run = service.run().await;

    assert_eq!(run.code, 0);
    assert_subset(&run.out, &case["expected_output"]);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        Value::Array(request_trace(&requests)),
        case["expected_requests"]
    );
}

#[tokio::test]
async fn growth_idle_sequence_and_result_match_pinned_reference_contract() {
    let fixture = fixture();
    let case = &fixture["growth_idle"];
    let responses = &case["responses"];
    let server = MockServer::start().await;

    for (route, key) in [
        (
            "/v2/activity/growth/buddy/travel/status",
            "travel_status",
        ),
        ("/v2/activity/growth/tasks", "tasks"),
        ("/v2/activity/growth/streak", "streak"),
        (
            "/v2/activity/growth/redeem/summary",
            "redeem_summary",
        ),
        (
            "/v2/activity/growth/lottery/chances",
            "lottery_chances",
        ),
        ("/v2/activity/growth/buddy/quota", "buddy_quota"),
        ("/v2/activity/growth/energy", "energy"),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(responses[key].clone()))
            .expect(1)
            .mount(&server)
            .await;
    }

    let budget = Arc::new(Budget::with_limit(std::time::Duration::from_secs(5)));
    let service = GrowthService::new(GrowthApi::new(client(&server, budget.clone())), budget);
    let run = service.run().await;

    assert_eq!(run.code, 0);
    assert_subset(&run.out, &case["expected_output"]);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        Value::Array(request_trace(&requests)),
        case["expected_requests"]
    );
}
