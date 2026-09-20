use serde_json::{json, Value};

use crate::http::{HttpResult, WorkBuddyClient};

pub const GROWTH_BASE: &str = "/v2/activity/growth";

#[derive(Clone)]
pub struct GrowthApi {
    client: WorkBuddyClient,
}

impl GrowthApi {
    pub fn new(client: WorkBuddyClient) -> Self {
        Self { client }
    }

    fn path(suffix: &str) -> String {
        format!("{GROWTH_BASE}{suffix}")
    }

    pub async fn travel_status(&self) -> HttpResult {
        self.client.get(&Self::path("/buddy/travel/status")).await
    }

    pub async fn travel_claim(&self, record_id: Value) -> HttpResult {
        let payload = json!({"record_id": record_id});
        self.client
            .post_once(&Self::path("/buddy/travel/claim"), Some(&payload))
            .await
    }

    pub async fn travel_config(&self) -> HttpResult {
        self.client.get(&Self::path("/buddy/travel/config")).await
    }

    pub async fn travel_depart(&self, location_id: Value) -> HttpResult {
        let payload = json!({"location_id": location_id});
        self.client
            .post_once(&Self::path("/buddy/travel/depart"), Some(&payload))
            .await
    }

    pub async fn tasks(&self) -> HttpResult {
        self.client.get(&Self::path("/tasks")).await
    }

    pub async fn accept_tasks(&self, task_codes: &[String]) -> HttpResult {
        let payload = json!({"task_codes": task_codes});
        self.client
            .post_once(&Self::path("/tasks/accept"), Some(&payload))
            .await
    }

    pub async fn claim_task(&self, task_code: &str) -> HttpResult {
        let payload = json!({});
        self.client
            .post_once(
                &Self::path(&format!("/tasks/{task_code}/claim")),
                Some(&payload),
            )
            .await
    }

    pub async fn streak(&self) -> HttpResult {
        self.client.get(&Self::path("/streak")).await
    }

    pub async fn use_makeup_card(
        &self,
        target_date: Value,
        client_token: String,
    ) -> HttpResult {
        let payload = json!({
            "target_date": target_date,
            "client_token": client_token
        });
        self.client
            .post_once(&Self::path("/makeup-cards/use"), Some(&payload))
            .await
    }

    pub async fn redeem_summary(&self) -> HttpResult {
        self.client.get(&Self::path("/redeem/summary")).await
    }

    pub async fn redeem(&self, tier: Value, client_token: String) -> HttpResult {
        let payload = json!({
            "tier": tier,
            "client_token": client_token
        });
        self.client
            .post_once(&Self::path("/redeem"), Some(&payload))
            .await
    }

    pub async fn lottery_chances(&self) -> HttpResult {
        self.client.get(&Self::path("/lottery/chances")).await
    }

    pub async fn lottery_draw(&self, client_token: String) -> HttpResult {
        let payload = json!({"client_token": client_token});
        self.client
            .post_once(&Self::path("/lottery/draw"), Some(&payload))
            .await
    }

    pub async fn buddy_quota(&self) -> HttpResult {
        self.client.get(&Self::path("/buddy/quota")).await
    }

    pub async fn buddy_open(&self, count: i64, client_token: String) -> HttpResult {
        let payload = json!({
            "count": count,
            "client_token": client_token
        });
        self.client
            .post_once(&Self::path("/buddy/open"), Some(&payload))
            .await
    }

    pub async fn energy(&self) -> HttpResult {
        self.client.get(&Self::path("/energy")).await
    }
}
