use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ServiceRun {
    pub code: i32,
    pub out: Value,
}
