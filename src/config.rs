pub const DEFAULT_ENDPOINT: &str = "https://copilot.tencent.com";

pub const DEFAULT_BUDGET_SECONDS: f64 = 420.0;
pub const MAX_BUDGET_SECONDS: f64 = 540.0;
pub const POLL_BUDGET_SECONDS: f64 = 180.0;
pub const POLL_MAX_BUDGET_SECONDS: f64 = 240.0;

pub const REQUEST_TIMEOUT_SECS: f64 = 30.0;
pub const NETWORK_RETRY_DELAYS: &[u64] = &[5, 15, 30, 60, 90];
pub const SERVER_RETRY_DELAYS: &[u64] = &[3, 10];

pub const MAKEUP_MAX_PER_RUN: usize = 1;

pub const AUTH_BASENAME: &[&str] = &[
    "CodeBuddyExtension",
    "Data",
    "Public",
    "auth",
    "workbuddy-desktop.info",
];
pub const CLI_AUTH_BASENAME: &[&str] = &[
    "CodeBuddyExtension",
    "Data",
    "Public",
    "auth",
    "Tencent-Cloud.coding-copilot.info",
];
