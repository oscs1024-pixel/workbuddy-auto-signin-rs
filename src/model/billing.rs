use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct CheckinStatusView {
    pub active: Option<bool>,
    pub activity_name: Option<String>,
    pub today_checked_in: Option<bool>,
    pub today_credit: Option<Value>,
    pub streak_days: Option<Value>,
    pub total_credits: Option<Value>,
    pub is_streak_day: Option<bool>,
    pub next_streak_day: Option<Value>,
}
