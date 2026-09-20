use serde_json::Value;

pub const CODE_NO_NETWORK: i32 = -1;
pub const CODE_BUDGET_OUT: i32 = -2;

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
