use workbuddy_auto_signin::service::growth::is_no_chance;

#[test]
fn exhausted_lottery_chance_is_normal_business_state() {
    assert!(is_no_chance("insufficient lottery chance balance"));
    assert!(is_no_chance("not enough chance"));
    assert!(!is_no_chance("invalid request"));
}
