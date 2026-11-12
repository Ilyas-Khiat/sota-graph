use std::process::Command;

fn sota(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sota"))
        .args(args)
        .output()
        .expect("failed to run the sota binary")
}

#[test]
fn version_prints_name_and_crate_version() {
    let out = sota(&["--version"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), format!("sota {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_states_what_the_tool_is_for() {
    let out = sota(&["--help"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("with a source for every number"),
        "{stdout}"
    );
}

#[test]
fn no_arguments_shows_help_and_fails() {
    let out = sota(&[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage:"));
}
