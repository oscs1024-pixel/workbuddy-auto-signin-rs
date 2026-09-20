use workbuddy_auto_signin::service::growth::compute_open_count;

#[test]
fn buddy_open_count_respects_server_quota_and_zero_default() {
    assert_eq!(compute_open_count(5, 2), 2);
    assert_eq!(compute_open_count(5, 0), 1);
    assert_eq!(compute_open_count(1, 5), 1);
}
