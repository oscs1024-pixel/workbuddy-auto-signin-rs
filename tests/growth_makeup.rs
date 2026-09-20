use serde_json::json;
use workbuddy_auto_signin::service::growth::parse_makeup_cards;

#[test]
fn makeup_card_balance_accepts_object_and_scalar_shapes() {
    assert_eq!(
        parse_makeup_cards(Some(&json!({"balance": 2}))),
        2
    );
    assert_eq!(parse_makeup_cards(Some(&json!(3))), 3);
    assert_eq!(parse_makeup_cards(None), 0);
}
