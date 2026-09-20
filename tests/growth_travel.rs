use serde_json::json;
use workbuddy_auto_signin::util::format_eta;

#[test]
fn eta_uses_server_time_and_avoids_sixty_minute_display() {
    assert_eq!(
        format_eta(Some(&json!(3599)), Some(&json!(0))),
        "，约 1.0 小时后回"
    );
    assert_eq!(
        format_eta(Some(&json!(0)), Some(&json!(1))),
        "，已到达待领取"
    );
}
