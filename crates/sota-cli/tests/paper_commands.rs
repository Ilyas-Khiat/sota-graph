//! Offline tests for `sota fetch` and `sota tables`: nothing here reaches the network.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sota-ingest/tests/fixtures/latexml_tables.html"
);

fn sota(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sota"))
        .args(args)
        .output()
        .expect("failed to run the sota binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A cache root holding the fixture page as 2401.01234v3.
fn seeded_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let arxiv = dir.path().join("arxiv");
    fs::create_dir_all(&arxiv).unwrap();
    fs::copy(FIXTURE, arxiv.join("2401.01234v3.html")).unwrap();
    dir
}

fn write_list(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join("papers.txt");
    fs::write(&path, text).unwrap();
    path
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn tables_prints_every_table_with_its_link() {
    let cache = seeded_cache();
    let out = sota(&["--cache", s(cache.path()), "tables", "2401.01234v3"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("## Table 1 (S4.T1) https://arxiv.org/html/2401.01234v3#S4.T1"),
        "{text}"
    );
    assert!(text.contains("| Ours (large) | **177** |"), "{text}");
    assert!(text.contains("## Table 3 (S4.T3.t2)"), "{text}");
    assert!(text.contains("conversion errors"), "{text}"); // Table 5 is flagged
    assert!(text.contains("S4.T4"), "{text}"); // the image-only figure is reported
}

#[test]
fn tables_can_show_a_single_table() {
    let cache = seeded_cache();
    let out = sota(&[
        "--cache",
        s(cache.path()),
        "tables",
        "2401.01234v3",
        "--table",
        "S4.T2",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("| Path-based | NeuralLP | 0.240 |"), "{text}");
    assert!(text.contains("|  | DRUM | 0.343 |"), "{text}"); // covered by the row span
    assert!(!text.contains("S4.T1"), "{text}");
}

#[test]
fn tables_names_the_known_tables_when_one_is_missing() {
    let cache = seeded_cache();
    let out = sota(&[
        "--cache",
        s(cache.path()),
        "tables",
        "2401.01234v3",
        "--table",
        "S9.T9",
    ]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("S4.T1"), "{}", stderr(&out));
}

#[test]
fn tables_needs_a_cached_pinned_version() {
    let cache = seeded_cache();
    let out = sota(&["--cache", s(cache.path()), "tables", "2401.01234v9"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("sota fetch"), "{}", stderr(&out));

    let out = sota(&["--cache", s(cache.path()), "tables", "2401.01234"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("version"), "{}", stderr(&out));
}

#[test]
fn fetch_with_everything_cached_makes_no_request_and_reports_coverage() {
    let cache = seeded_cache();
    let list = write_list(cache.path(), "# comment\n\n2401.01234v3  # synthetic\n");
    let out = sota(&["--cache", s(cache.path()), "fetch", "--list", s(&list)]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("2401.01234v3") && text.contains("cached"),
        "{text}"
    );
    assert!(
        text.contains("listed 1 | html 1 | no html 0 | failed 0"),
        "{text}"
    );
    assert!(
        text.contains("table figures 6 | tables parsed 6 | figures without a table 1 | tables with holes 0 | tables with conversion errors 1"),
        "{text}"
    );
}

#[test]
fn fetch_refuses_a_list_with_unpinned_or_bad_ids_before_any_request() {
    let dir = tempfile::tempdir().unwrap();
    let list = write_list(dir.path(), "2401.01234v3\n2401.05678\n");
    let out = sota(&["--cache", s(dir.path()), "fetch", "--list", s(&list)]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("line 2"), "{}", stderr(&out));
    assert!(!dir.path().join("arxiv").exists(), "nothing may be fetched");

    let list = write_list(dir.path(), "https://arxiv.org/abs/2401.01234v3\n");
    let out = sota(&["--cache", s(dir.path()), "fetch", "--list", s(&list)]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("line 1"), "{}", stderr(&out));
}

#[test]
fn fetch_refuses_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let list = write_list(dir.path(), "2401.01234v3\n2401.01234v3\n");
    let out = sota(&["--cache", s(dir.path()), "fetch", "--list", s(&list)]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("duplicate"), "{}", stderr(&out));
}
