use workbuddy_auto_signin::config::{NETWORK_RETRY_DELAYS, SERVER_RETRY_DELAYS};
use workbuddy_auto_signin::http::{retry_delays, RetryClass, CODE_BUDGET_OUT, CODE_NO_NETWORK};

#[test]
fn network_and_server_use_independent_schedules() {
    let (network_class, network) = retry_delays(CODE_NO_NETWORK).unwrap();
    let (server_class, server) = retry_delays(500).unwrap();

    assert_eq!(network_class, RetryClass::Network);
    assert_eq!(network, NETWORK_RETRY_DELAYS);
    assert_eq!(server_class, RetryClass::Server);
    assert_eq!(server, SERVER_RETRY_DELAYS);
}

#[test]
fn business_errors_and_budget_do_not_retry() {
    assert!(retry_delays(400).is_none());
    assert!(retry_delays(401).is_none());
    assert!(retry_delays(CODE_BUDGET_OUT).is_none());
}


#[tokio::test]
async fn server_retry_obeys_budget_cutoff() {
    use std::sync::Arc;
    use std::time::Duration;

    use reqwest::header::HeaderMap;
    use serde_json::json;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use workbuddy_auto_signin::budget::Budget;
    use workbuddy_auto_signin::http::WorkBuddyClient;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/retry"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"msg":"boom"})))
        .expect(1)
        .mount(&server)
        .await;

    // 剩余预算不足 delay(3s)+完整请求超时(30s)，不得开始一次注定跑穿预算的重试。
    let client = WorkBuddyClient::new(
        Url::parse(&server.uri()).unwrap(),
        HeaderMap::new(),
        Arc::new(Budget::with_limit(Duration::from_secs(5))),
    )
    .unwrap();

    let result = client.get("/retry").await;
    assert_eq!(result.code, 500);
}
