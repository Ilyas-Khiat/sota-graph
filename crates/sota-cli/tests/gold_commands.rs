//! Offline tests for `sota gold check|expand|show`, on the synthetic fixture page.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sota-ingest/tests/fixtures/latexml_tables.html"
);
const CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");

const SPEC: &str = r#"
paper = "2401.01234v3"
title = "A Synthetic Paper on Link Prediction"

[[table]]
id = "S4.T1"
header_rows = 2
settings = { protocol = "filtered" }
evidence = "S1.p1: reports filtered metrics"
columns = [
  { col = 1, dataset = "fb15k-237", metric = "mr" },
  { col = 2, dataset = "fb15k-237", metric = "mrr" },
  { col = 3, dataset = "wn18rr", metric = "mr" },
  { col = 4, dataset = "wn18rr", metric = "mrr" },
]
rows = [
  { row = 2, method = "TransE", method_id = "transe", role = "baseline" },
  { row = 3, method = "Ours (large)", method_id = "ours", role = "proposed" },
]
"#;

struct Env {
    dir: tempfile::TempDir,
}

impl Env {
    /// A cache holding the fixture page, and a gold dir holding SPEC.
    fn new(spec: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arxiv = dir.path().join("cache/arxiv");
        fs::create_dir_all(&arxiv).unwrap();
        fs::copy(FIXTURE, arxiv.join("2401.01234v3.html")).unwrap();
        fs::create_dir_all(dir.path().join("gold/demo")).unwrap();
        let env = Self { dir };
        env.write_spec(spec);
        env
    }

    fn write_spec(&self, spec: &str) {
        fs::write(self.spec_path(), spec).unwrap();
    }

    fn spec_path(&self) -> PathBuf {
        self.dir.path().join("gold/demo/2401.01234v3.toml")
    }

    fn snapshot_path(&self) -> PathBuf {
        self.dir.path().join("gold/demo/2401.01234v3.results.jsonl")
    }

    fn drop_cache(&self) {
        fs::remove_dir_all(self.dir.path().join("cache")).unwrap();
    }

    fn sota(&self, args: &[&str]) -> Output {
        let root = self.dir.path();
        let mut all = vec![
            "--config".to_owned(),
            CONFIG.to_owned(),
            "--cache".to_owned(),
            s(&root.join("cache")),
        ];
        all.extend(args.iter().map(|a| a.to_string()));
        all.push("--dir".to_owned());
        all.push(s(&root.join("gold")));
        Command::new(env!("CARGO_BIN_EXE_sota"))
            .args(&all)
            .output()
            .expect("failed to run sota")
    }
}

fn s(p: &Path) -> String {
    p.to_str().unwrap().to_owned()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn expand_writes_a_snapshot_that_check_accepts() {
    let env = Env::new(SPEC);
    let o = env.sota(&["gold", "expand"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("7 results, 1 missing"), "{}", out(&o));

    let snapshot = fs::read_to_string(env.snapshot_path()).unwrap();
    assert_eq!(snapshot.lines().count(), 7);
    assert!(
        snapshot.contains(r#""locator":"2401.01234v3#S4.T1/r3/c2""#),
        "{snapshot}"
    );
    assert!(
        snapshot.contains(r#""value":0.338,"std":0.002"#),
        "{snapshot}"
    );

    let o = env.sota(&["gold", "check"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("re-expanded from cache"), "{}", out(&o));
}

#[test]
fn check_without_a_snapshot_fails() {
    let env = Env::new(SPEC);
    let o = env.sota(&["gold", "check"]);
    assert!(!o.status.success());
    assert!(out(&o).contains("gold expand"), "{}", out(&o));
}

#[test]
fn check_without_the_cache_still_checks_the_snapshot() {
    let env = Env::new(SPEC);
    assert!(env.sota(&["gold", "expand"]).status.success());
    env.drop_cache();
    let o = env.sota(&["gold", "check"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("not cached"), "{}", out(&o));

    // An edit to the spec after expanding is caught even without the paper.
    env.write_spec(&SPEC.replace(r#"role = "baseline""#, r#"role = "variant""#));
    let o = env.sota(&["gold", "check"]);
    assert!(!o.status.success());
    assert!(out(&o).contains("role differs"), "{}", out(&o));
}

#[test]
fn check_with_the_cache_catches_a_stale_snapshot() {
    let env = Env::new(SPEC);
    assert!(env.sota(&["gold", "expand"]).status.success());
    // Drop one line: still consistent with the spec, but not what the paper gives.
    let snapshot = fs::read_to_string(env.snapshot_path()).unwrap();
    let shorter: Vec<&str> = snapshot.lines().skip(1).collect();
    fs::write(env.snapshot_path(), shorter.join("\n") + "\n").unwrap();
    let o = env.sota(&["gold", "check"]);
    assert!(!o.status.success());
    assert!(out(&o).contains("stale"), "{}", out(&o));
}

#[test]
fn expand_refuses_a_mapping_the_paper_contradicts() {
    let env = Env::new(&SPEC.replace(
        r#"metric = "mrr" },
  { col = 3"#,
        r#"metric = "hits@10" },
  { col = 3"#,
    ));
    let o = env.sota(&["gold", "expand"]);
    assert!(!o.status.success());
    assert!(
        out(&o).contains("does not name metric hits@10"),
        "{}",
        out(&o)
    );
    assert!(!env.snapshot_path().exists());
}

#[test]
fn show_renders_the_mapping_for_review() {
    let env = Env::new(SPEC);
    let o = env.sota(&["gold", "show", "2401.01234v3"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(
        text.contains("https://arxiv.org/html/2401.01234v3#S4.T1"),
        "{text}"
    );
    assert!(text.contains("protocol=filtered"), "{text}");
    assert!(text.contains("fb15k-237 mrr"), "{text}");
    assert!(
        text.contains("| Ours (large) | ours (proposed) | 177 | 0.338\\pm 0.002 | - | .476 |"),
        "{text}"
    );
}

#[test]
fn a_bad_spec_file_names_itself() {
    let env = Env::new("paper = \n");
    let o = env.sota(&["gold", "check"]);
    assert!(!o.status.success());
    assert!(
        out(&o).contains("2401.01234v3.toml"),
        "{}{}",
        out(&o),
        err(&o)
    );
}

// ------------------------------------------------ the shipped gold set

const REPO_GOLD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../gold");
const REPO_CACHE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cache");

fn shipped_gold_check(cache: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sota"))
        .args([
            "--config", CONFIG, "--cache", cache, "gold", "check", "--dir", REPO_GOLD,
        ])
        .output()
        .expect("failed to run sota")
}

/// Runs in CI, where there is no paper cache: every committed snapshot must
/// agree with its spec, and the gold set must not silently shrink.
#[test]
fn the_shipped_gold_set_is_consistent_without_the_papers() {
    let empty = tempfile::tempdir().unwrap();
    let o = shipped_gold_check(empty.path().to_str().unwrap());
    let text = out(&o);
    assert!(o.status.success(), "{text}{}", err(&o));
    let summary = text.lines().last().unwrap_or_default();
    let specs: usize = summary.split(' ').next().unwrap().parse().unwrap();
    let results: usize = summary
        .split(", ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(specs >= 10, "{summary}");
    assert!(results >= 900, "{summary}");
}

/// With the real cache: every spec is re-expanded from its paper (labels and
/// headers re-checked) and must equal its snapshot.
#[test]
#[ignore = "needs the local paper cache"]
fn the_shipped_gold_set_re_expands_from_the_papers() {
    let o = shipped_gold_check(REPO_CACHE);
    let text = out(&o);
    assert!(o.status.success(), "{text}{}", err(&o));
    let specs = text.lines().filter(|l| l.contains(" ok (")).count();
    let re_expanded = text
        .lines()
        .filter(|l| l.contains("re-expanded from cache"))
        .count();
    assert_eq!(specs, re_expanded, "{text}");
}
