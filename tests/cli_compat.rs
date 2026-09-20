use workbuddy_auto_signin::cli::Action;

#[test]
fn legacy_silent_growth_remains_supported() {
    assert_eq!(
        Action::parse("silent-growth"),
        Some(Action::SilentGrowth)
    );
    assert!(Action::SilentGrowth.is_poll());
}

#[test]
fn documented_commands_are_all_parseable() {
    for action in Action::NAMES {
        assert!(Action::parse(action).is_some(), "{action}");
    }
}
