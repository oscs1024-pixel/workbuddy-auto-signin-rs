use crate::config::{NETWORK_RETRY_DELAYS, SERVER_RETRY_DELAYS};

use super::response::{CODE_BUDGET_OUT, CODE_NO_NETWORK};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum RetryClass {
    Network,
    Server,
}

pub fn retry_delays(code: i32) -> Option<(RetryClass, &'static [u64])> {
    if code == CODE_NO_NETWORK {
        Some((RetryClass::Network, NETWORK_RETRY_DELAYS))
    } else if code >= 500 {
        Some((RetryClass::Server, SERVER_RETRY_DELAYS))
    } else {
        None
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
