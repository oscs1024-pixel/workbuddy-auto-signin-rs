pub mod env;
pub mod json;
pub mod number;
pub mod time;

pub use env::env_flag;
pub use json::{dig, first_i64, is_true_or_one, value_string, value_truthy};
pub use number::{as_i64, format_credit, try_i64};
pub use time::format_eta;

pub fn client_token(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4())
}
