use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::Method;
use serde_json::{json, Value};
use url::Url;

use crate::budget::Budget;
use crate::config::REQUEST_TIMEOUT_SECS;

use super::response::{HttpResult, CODE_BUDGET_OUT, CODE_NO_NETWORK};
use super::retry::{retry_delays, RetryClass};

#[derive(Clone)]
pub struct WorkBuddyClient {
    inner: reqwest::Client,
    endpoint: Url,
    headers: HeaderMap,
    budget: Arc<Budget>,
}

impl WorkBuddyClient {
    pub fn new(
        endpoint: Url,
        headers: HeaderMap,
        budget: Arc<Budget>,
    ) -> Result<Self, reqwest::Error> {
        let inner = reqwest::Client::builder().build()?;
        Ok(Self {
            inner,
            endpoint,
            headers,
            budget,
        })
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.endpoint.as_str().trim_end_matches('/'), path)
    }

    async fn send_once(
        &self,
        method: Method,
        path: &str,
        payload: Option<&Value>,
        timeout: Duration,
    ) -> HttpResult {
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
                return HttpResult {
                    code: CODE_NO_NETWORK,
                    body: json!({"error": error.to_string()}),
                };
            }
        };

        let code = response.status().as_u16() as i32;
        let raw = match response.text().await {
            Ok(raw) => raw,
            Err(error) => {
                return HttpResult {
                    code: CODE_NO_NETWORK,
                    body: json!({"error": error.to_string()}),
                };
            }
        };

        let body = serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| {
            let truncated: String = raw.chars().take(500).collect();
            json!({"raw": truncated})
        });

        HttpResult { code, body }
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        payload: Option<&Value>,
        retry: bool,
    ) -> HttpResult {
        if self.budget.remaining_secs_f64() <= 1.0 {
            return HttpResult {
                code: CODE_BUDGET_OUT,
                body: json!({"error": "已达本次运行时间预算，跳过剩余请求"}),
            };
        }

        let timeout = Duration::from_secs_f64(
            self.budget
                .remaining_secs_f64()
                .min(REQUEST_TIMEOUT_SECS)
                .max(1.0),
        );

        let mut result = self.send_once(method.clone(), path, payload, timeout).await;
        if !retry {
            return result;
        }

        let mut attempts: HashMap<RetryClass, usize> = HashMap::new();
        loop {
            let Some((class, delays)) = retry_delays(result.code) else {
                return result;
            };
            let used = *attempts.get(&class).unwrap_or(&0);
            if used >= delays.len() {
                return result;
            }

            let delay = delays[used];
            attempts.insert(class, used + 1);

            if self.budget.remaining_secs_f64()
                <= delay as f64 + REQUEST_TIMEOUT_SECS
            {
                return result;
            }

            tokio::time::sleep(Duration::from_secs(delay)).await;

            let timeout = Duration::from_secs_f64(
                self.budget
                    .remaining_secs_f64()
                    .min(REQUEST_TIMEOUT_SECS)
                    .max(1.0),
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

    pub async fn post_retryable(
        &self,
        path: &str,
        payload: Option<&Value>,
    ) -> HttpResult {
        self.request(Method::POST, path, payload, true).await
    }
}
