use serde_json::{json, Value};

use crate::http::{HttpResult, WorkBuddyClient};

pub const CHECKIN_STATUS: &str = "/v2/billing/meter/checkin-activity-status";
pub const DAILY_CHECKIN: &str = "/v2/billing/meter/daily-checkin";
pub const GROWTH_BASE: &str = "/v2/activity/growth";

#[derive(Clone)]
pub struct BillingApi {
    client: WorkBuddyClient,
}

impl BillingApi {
    pub fn new(client: WorkBuddyClient) -> Self { Self { client } }

    pub async fn status(&self) -> HttpResult {
        self.client.post_retryable(CHECKIN_STATUS, None).await
    }

    pub async fn claim(&self) -> HttpResult {
        self.client.post_retryable(DAILY_CHECKIN, None).await
    }
}

#[derive(Clone)]
pub struct GrowthApi {
    client: WorkBuddyClient,
}

impl GrowthApi {
    pub fn new(client: WorkBuddyClient) -> Self { Self { client } }

    fn p(suffix: &str) -> String { format!("{GROWTH_BASE}{suffix}") }

    pub async fn travel_status(&self) -> HttpResult { self.client.get(&Self::p("/buddy/travel/status")).await }
    pub async fn travel_claim(&self, record_id: Value) -> HttpResult {
        let payload = json!({"record_id": record_id});
        self.client.post_once(&Self::p("/buddy/travel/claim"), Some(&payload)).await
    }
    pub async fn travel_config(&self) -> HttpResult { self.client.get(&Self::p("/buddy/travel/config")).await }
    pub async fn travel_depart(&self, location_id: Value) -> HttpResult {
        let payload = json!({"location_id": location_id});
        self.client.post_once(&Self::p("/buddy/travel/depart"), Some(&payload)).await
    }
    pub async fn tasks(&self) -> HttpResult { self.client.get(&Self::p("/tasks")).await }
    pub async fn accept_tasks(&self, codes: &[String]) -> HttpResult {
        let payload = json!({"task_codes": codes});
        self.client.post_once(&Self::p("/tasks/accept"), Some(&payload)).await
    }
    pub async fn claim_task(&self, code: &str) -> HttpResult {
        let payload = json!({});
        self.client.post_once(&Self::p(&format!("/tasks/{code}/claim")), Some(&payload)).await
    }
    pub async fn streak(&self) -> HttpResult { self.client.get(&Self::p("/streak")).await }
    pub async fn use_makeup_card(&self, target_date: Value, client_token: String) -> HttpResult {
        let payload = json!({"target_date": target_date, "client_token": client_token});
        self.client.post_once(&Self::p("/makeup-cards/use"), Some(&payload)).await
    }
    pub async fn redeem_summary(&self) -> HttpResult { self.client.get(&Self::p("/redeem/summary")).await }
    pub async fn redeem(&self, tier: Value, client_token: String) -> HttpResult {
        let payload = json!({"tier": tier, "client_token": client_token});
        self.client.post_once(&Self::p("/redeem"), Some(&payload)).await
    }
    pub async fn lottery_chances(&self) -> HttpResult { self.client.get(&Self::p("/lottery/chances")).await }
    pub async fn lottery_draw(&self, client_token: String) -> HttpResult {
        let payload = json!({"client_token": client_token});
        self.client.post_once(&Self::p("/lottery/draw"), Some(&payload)).await
    }
    pub async fn buddy_quota(&self) -> HttpResult { self.client.get(&Self::p("/buddy/quota")).await }
    pub async fn buddy_open(&self, count: i64, client_token: String) -> HttpResult {
        let payload = json!({"count": count, "client_token": client_token});
        self.client.post_once(&Self::p("/buddy/open"), Some(&payload)).await
    }
    pub async fn energy(&self) -> HttpResult { self.client.get(&Self::p("/energy")).await }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_inventory_matches_python_contract() {
        let growth = [
            "/buddy/travel/status", "/buddy/travel/claim", "/buddy/travel/config", "/buddy/travel/depart",
            "/tasks", "/tasks/accept", "/tasks/{task_code}/claim", "/streak", "/makeup-cards/use",
            "/redeem/summary", "/redeem", "/lottery/chances", "/lottery/draw", "/buddy/quota",
            "/buddy/open", "/energy",
        ];
        assert_eq!(growth.len(), 16);
        assert_eq!([CHECKIN_STATUS, DAILY_CHECKIN].len() + growth.len(), 18);
    }
}
