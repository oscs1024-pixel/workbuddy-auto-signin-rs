use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct TaskItem {
    pub task_code: Option<String>,
    pub title: Option<String>,
    pub locked: Option<bool>,
    pub accept_status: Option<String>,
    pub reward_credit: Option<Value>,
    pub reward_energy: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskAcceptResult {
    pub task_code: Option<String>,
    pub status: Option<String>,
    pub message: Option<Value>,
}
