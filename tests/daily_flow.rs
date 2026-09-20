use workbuddy_auto_signin::service::daily::is_quiet;

#[test]
fn poll_is_quiet_only_for_already_or_inactive_idle_runs() {
    assert!(is_quiet("ALREADY", true));
    assert!(is_quiet("INACTIVE", true));
    assert!(!is_quiet("CLAIMED", true));
    assert!(!is_quiet("ALREADY", false));
}
