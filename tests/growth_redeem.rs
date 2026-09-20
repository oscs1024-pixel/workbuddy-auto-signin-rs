use serde_json::json;

use workbuddy_auto_signin::http::HttpResult;
use workbuddy_auto_signin::service::growth::{
    is_tier_locked, is_unknown_tier, redeem_reward_desc,
};

#[test]
fn redeem_uses_granted_fields_for_actual_reward() {
    assert_eq!(
        redeem_reward_desc(
            &json!({
                "credit_granted": 50,
                "energy_granted": 3,
                "cards_granted": 1,
                "chances_granted": 1
            }),
            "14d"
        ),
        "（+50 积分 +3 能量 +1 补登卡 +1 次抽奖）"
    );
}

#[test]
fn tier_contract_distinguishes_fallback_and_locked_state() {
    assert!(is_unknown_tier(&HttpResult {
        code: 400,
        body: json!({"msg": "unknown tier"}),
    }));

    assert!(!is_unknown_tier(&HttpResult {
        code: 400,
        body: json!({"msg": "invalid request"}),
    }));

    assert!(is_tier_locked(&HttpResult {
        code: 403,
        body: json!({"msg": "连续登录天数不足"}),
    }));
}
