use tempfile::tempdir;
use workbuddy_auto_signin::auth::discover_from_candidates;

#[test]
fn explicit_auth_path_has_priority_and_no_fallback() {
    let dir = tempdir().unwrap();
    let explicit = dir.path().join("explicit.info");
    std::fs::write(&explicit, "{}").unwrap();

    let fallback = dir.path().join("fallback.info");
    std::fs::write(&fallback, "{}").unwrap();

    let result = discover_from_candidates(Some(explicit.clone()), vec![fallback]);

    assert_eq!(result.found, Some(explicit.clone()));
    assert_eq!(result.looked_in, vec![explicit]);
}

#[test]
fn missing_explicit_path_does_not_fallback() {
    let dir = tempdir().unwrap();
    let explicit = dir.path().join("missing.info");
    let fallback = dir.path().join("fallback.info");
    std::fs::write(&fallback, "{}").unwrap();

    let result = discover_from_candidates(Some(explicit.clone()), vec![fallback]);

    assert!(result.found.is_none());
    assert_eq!(result.looked_in, vec![explicit]);
}
