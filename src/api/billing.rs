use crate::http::{HttpResult, WorkBuddyClient};

pub const CHECKIN_STATUS: &str = "/v2/billing/meter/checkin-activity-status";
pub const DAILY_CHECKIN: &str = "/v2/billing/meter/daily-checkin";

#[derive(Clone)]
pub struct BillingApi {
    client: WorkBuddyClient,
}

impl BillingApi {
    pub fn new(client: WorkBuddyClient) -> Self {
        Self { client }
    }

    pub async fn status(&self) -> HttpResult {
        self.client.post_retryable(CHECKIN_STATUS, None).await
    }

    pub async fn claim(&self) -> HttpResult {
        self.client.post_retryable(DAILY_CHECKIN, None).await
    }
}
