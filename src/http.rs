use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::Method;
use serde_json::{json, Value};
use url::Url;

use crate::budget::Budget;

pub const CODE_NO_NETWORK: i32 = -1;
pub const CODE_BUDGET_OUT: i32 = -2;
pub const REQUEST_TIMEOUT_SECS: f64 = 30.0;
pub const NETWORK_RETRY_DELAYS: &[u64] = &[5, 15, 30, 60, 90];
pub const SERVER_RETRY_DELAYS: &[u64] = &[3, 10];

#[derive(Debug, Clone)]
pub struct HttpResult {
    pub code: i32,
    pub body: Value,
}

impl HttpResult {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.code)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum RetryClass {
    Network,
    Server,
}

#[derive(Clone)]
pub struct WorkBuddyClient {
    inner: reqwest::Client,
    endpoint: Url,
    headers: HeaderMap,
    budget: Arc<Budget>,
}

impl WorkBuddyClient {
    pub fn new(endpoint: Url, headers: HeaderMap, budget: Arc<Budget>) -> Result<Self, reqwest::Error> {
        let inner = reqwest::Client::builder().build()?;
        Ok(Self { inner, endpoint, headers, budget })
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.endpoint.as_str().trim_end_matches('/'), path)
    }

    async fn send_once(&self, method: Method, path: &str, payload: Option<&Value>, timeout: Duration) -> HttpResult {
        let mut request = self
            .inner
            .request(method, self.url(path))
            .headers(self.headers.clone())
            .timeout(timeout);
        if let Some(payload) = payload {
            request = request.json(payload);
        }

        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                return HttpResult { code: CODE_NO_NETWORK, body: json!({"error": error.to_string()}) };
            }
        };
        let code = response.status().as_u16() as i32;
        let raw = match response.text().await {
            Ok(raw) => raw,
            Err(error) => {
                return HttpResult { code: CODE_NO_NETWORK, body: json!({"error": error.to_string()}) };
            }
        };
        let body = serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| {
            let truncated: String = raw.chars().take(500).collect();
            json!({"raw": truncated})
        });
        HttpResult { code, body }
    }

    fn retry_schedule(code: i32) -> Option<(RetryClass, &'static [u64])> {
        if code == CODE_NO_NETWORK {
            Some((RetryClass::Network, NETWORK_RETRY_DELAYS))
        } else if code >= 500 {
            Some((RetryClass::Server, SERVER_RETRY_DELAYS))
        } else {
            None
        }
    }

    async fn request(&self, method: Method, path: &str, payload: Option<&Value>, retry: bool) -> HttpResult {
        if self.budget.remaining_secs_f64() <= 1.0 {
            return HttpResult {
                code: CODE_BUDGET_OUT,
                body: json!({"error": "已达本次运行时间预算，跳过剩余请求"}),
            };
        }

        let timeout = Duration::from_secs_f64(
            self.budget.remaining_secs_f64().min(REQUEST_TIMEOUT_SECS).max(1.0),
        );
        let mut result = self.send_once(method.clone(), path, payload, timeout).await;
        if !retry {
            return result;
        }

        let mut attempts: HashMap<RetryClass, usize> = HashMap::new();
        loop {
            let Some((class, delays)) = Self::retry_schedule(result.code) else {
                return result;
            };
            let used = *attempts.get(&class).unwrap_or(&0);
            if used >= delays.len() {
                return result;
            }
            let delay = delays[used];
            attempts.insert(class, used + 1);
            if self.budget.remaining_secs_f64() <= delay as f64 + REQUEST_TIMEOUT_SECS {
                return result;
            }
            tokio::time::sleep(Duration::from_secs(delay)).await;
            let timeout = Duration::from_secs_f64(
                self.budget.remaining_secs_f64().min(REQUEST_TIMEOUT_SECS).max(1.0),
            );
            result = self.send_once(method.clone(), path, payload, timeout).await;
        }
    }

    pub async fn get(&self, path: &str) -> HttpResult {
        self.request(Method::GET, path, None, true).await
    }

    pub async fn post_once(&self, path: &str, payload: Option<&Value>) -> HttpResult {
        self.request(Method::POST, path, payload, false).await
    }

    pub async fn post_retryable(&self, path: &str, payload: Option<&Value>) -> HttpResult {
        self.request(Method::POST, path, payload, true).await
    }
}

pub fn is_hard_failure(code: i32) -> bool {
    code >= 500 || matches!(code, CODE_NO_NETWORK | CODE_BUDGET_OUT)
}

pub fn http_label(code: i32) -> String {
    match code {
        CODE_NO_NETWORK => "网络不可达".to_string(),
        CODE_BUDGET_OUT => "时间预算耗尽".to_string(),
        _ => format!("HTTP {code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_json_and_non_json_bodies() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .mount(&server).await;
        Mock::given(method("POST")).and(path("/raw"))
            .respond_with(ResponseTemplate::new(500).set_body_string("oops"))
            .mount(&server).await;

        let budget = Arc::new(Budget::for_action(None).0);
        let client = WorkBuddyClient::new(Url::parse(&server.uri()).unwrap(), HeaderMap::new(), budget).unwrap();
        let ok = client.get("/json").await;
        assert_eq!(ok.code, 200);
        assert_eq!(ok.body["ok"], true);
        let raw = client.post_once("/raw", None).await;
        assert_eq!(raw.code, 500);
        assert_eq!(raw.body["raw"], "oops");
    }
}
