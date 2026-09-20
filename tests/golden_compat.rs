use serde_json::Value;

use workbuddy_auto_signin::service::growth::{
    compute_open_count, is_no_chance, redeem_reward_desc,
};
use workbuddy_auto_signin::service::signin::already_report;
use workbuddy_auto_signin::util::format_eta;

#[test]
fn verified_contract_golden_scenarios_remain_stable() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/golden_scenarios.json")).unwrap();

    let signin = &fixture["signin_already"];
    assert_eq!(already_report(&signin["status"], None), signin["expected"]);

    let redeem = &fixture["redeem_reward_14d"];
    assert_eq!(
        redeem_reward_desc(&redeem["body"], "14d"),
        redeem["expected"].as_str().unwrap()
    );

    let eta = &fixture["eta"];
    assert_eq!(
        format_eta(Some(&eta["arrive_at"]), Some(&eta["server_now"])),
        eta["expected"].as_str().unwrap()
    );

    let lottery = &fixture["lottery_no_chance"];
    assert_eq!(
        is_no_chance(lottery["message"].as_str().unwrap()),
        lottery["expected"].as_bool().unwrap()
    );

    let buddy = &fixture["buddy_count"];
    assert_eq!(
        compute_open_count(
            buddy["affordable"].as_i64().unwrap(),
            buddy["max_open_count"].as_i64().unwrap()
        ),
        buddy["expected"].as_i64().unwrap()
    );
}
