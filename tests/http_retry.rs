use workbuddy_auto_signin::config::{
    NETWORK_RETRY_DELAYS, SERVER_RETRY_DELAYS,
};
use workbuddy_auto_signin::http::{
    retry_delays, RetryClass, CODE_BUDGET_OUT, CODE_NO_NETWORK,
};

#[test]
fn network_and_server_use_independent_schedules() {
    let (network_class, network) =
        retry_delays(CODE_NO_NETWORK).unwrap();
    let (server_class, server) = retry_delays(500).unwrap();

    assert_eq!(network_class, RetryClass::Network);
    assert_eq!(network, NETWORK_RETRY_DELAYS);
    assert_eq!(server_class, RetryClass::Server);
    assert_eq!(server, SERVER_RETRY_DELAYS);
}

#[test]
fn business_errors_and_budget_do_not_retry() {
    assert!(retry_delays(400).is_none());
    assert!(retry_delays(401).is_none());
    assert!(retry_delays(CODE_BUDGET_OUT).is_none());
}
