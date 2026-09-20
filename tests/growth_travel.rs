use serde_json::json;
use workbuddy_auto_signin::util::format_eta;

#[test]
fn eta_uses_server_time_and_displays_countdown() {
    assert_eq!(
        format_eta(Some(&json!(10256)), Some(&json!(0))),
        "，旅行倒计时 02:50:56"
    );
    assert_eq!(
        format_eta(Some(&json!(0)), Some(&json!(1))),
        "，已到达待领取"
    );
}
