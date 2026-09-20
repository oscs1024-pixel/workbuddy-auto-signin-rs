use std::process::Command;

use tempfile::tempdir;

#[test]
fn no_arguments_are_equivalent_to_auto() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("missing.info");
    let exe = env!("CARGO_BIN_EXE_workbuddy-auto-signin");

    let run = |args: &[&str]| {
        Command::new(exe)
            .args(args)
            .env("WORKBUDDY_AUTH_FILE", &missing)
            .env("WORKBUDDY_OUTPUT", "json")
            .output()
            .unwrap()
    };

    let default = run(&[]);
    let explicit = run(&["auto"]);

    assert_eq!(default.status.code(), Some(2));
    assert_eq!(explicit.status.code(), Some(2));
    assert_eq!(default.stdout, explicit.stdout);
}
