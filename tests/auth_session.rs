use serde_json::json;

use workbuddy_auto_signin::auth::build_session_context;
use workbuddy_auto_signin::error::AuthError;
use workbuddy_auto_signin::model::auth::SessionFile;

#[test]
fn explicit_null_auth_and_account_are_treated_as_empty_objects() {
    let session: SessionFile = serde_json::from_value(json!({
        "auth": null,
        "account": null
    }))
    .unwrap();

    assert!(matches!(
        build_session_context(&session),
        Err(AuthError::NoSession)
    ));
}
