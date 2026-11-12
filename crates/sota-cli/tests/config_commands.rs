use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn shipped_config() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config"))
}

fn sota(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sota"))
        .args(args)
        .output()
        .expect("failed to run the sota binary")
}

fn with_config(args: &[&str]) -> Output {
    let dir = shipped_config();
    let mut all = vec!["--config", dir.to_str().unwrap()];
    all.extend_from_slice(args);
    sota(&all)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn check_config_accepts_the_shipped_config() {
    let out = with_config(&["check-config"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("5 node types, 6 relationship types"),
        "{text}"
    );
    assert!(text.contains("kg-completion"), "{text}");
    assert!(text.trim_end().ends_with("OK"), "{text}");
}

#[test]
fn check_config_reports_problems_and_fails() {
    let dir = tempfile::tempdir().unwrap();
    let shipped = shipped_config();
    fs::copy(
        shipped.join("ontology.toml"),
        dir.path().join("ontology.toml"),
    )
    .unwrap();
    fs::copy(
        shipped.join("normalization.toml"),
        dir.path().join("normalization.toml"),
    )
    .unwrap();
    fs::create_dir(dir.path().join("registry")).unwrap();
    fs::write(
        dir.path().join("registry/bad.toml"),
        "topic = \"bad\"\n[[dataset]]\nid = \"a\"\nname = \"Same\"\n[[dataset]]\nid = \"b\"\nname = \"same\"\n",
    )
    .unwrap();

    let out = sota(&["--config", dir.path().to_str().unwrap(), "check-config"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("Same") && err.contains("same"), "{err}");
}

#[test]
fn missing_config_dir_fails_with_the_path() {
    let out = sota(&["--config", "does/not/exist", "check-config"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("exist"), "{}", stderr(&out));
}

#[test]
fn resolve_prints_the_canonical_entry() {
    let out = with_config(&["resolve", "dataset", "FB15K 237"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "fb15k-237\tFB15k-237");

    let out = with_config(&["resolve", "metric", "H@10"]);
    assert_eq!(stdout(&out).trim(), "hits@10\tHits@10");
}

#[test]
fn resolve_reports_unknown_mentions_with_their_key() {
    let out = with_config(&["resolve", "dataset", "Free Base"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.starts_with("unknown"), "{text}");
    assert!(text.contains("freebase"), "{text}");
}
