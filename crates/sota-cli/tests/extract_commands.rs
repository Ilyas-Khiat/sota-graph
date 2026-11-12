//! Offline tests for `sota extract` and `sota eval extract`. Answers come from
//! recordings written here; nothing calls a model, and `--live` is only
//! exercised with no credentials in a directory without a `.env`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sota_core::{Normalizer, Registry};
use sota_extract::llm::recording_key;
use sota_extract::prompt::request_for;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sota-ingest/tests/fixtures/latexml_tables.html"
);
const CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
const MODEL: &str = "test-model/none";

const S4_T1: &str = r#"{
  "is_results_table": true, "orientation": "methods_in_rows", "header_rows": 2, "label_col": 0,
  "settings": [{ "setting": "protocol", "value": "filtered", "evidence": "S1.p1" }],
  "columns": [
    { "col": 1, "metric": "mr", "dataset": "fb15k-237", "method": null, "role": null, "scale": "raw", "settings": [] },
    { "col": 2, "metric": "mrr", "dataset": "fb15k-237", "method": null, "role": null, "scale": "fraction", "settings": [] },
    { "col": 3, "metric": "mr", "dataset": "wn18rr", "method": null, "role": null, "scale": "raw", "settings": [] },
    { "col": 4, "metric": "mrr", "dataset": "wn18rr", "method": null, "role": null, "scale": "fraction", "settings": [] }
  ],
  "rows": [
    { "row": 2, "method": "TransE", "role": "baseline", "dataset": null },
    { "row": 3, "method": "Ours (large)", "role": "proposed", "dataset": null }
  ],
  "review": []
}"#;

const NOT_RESULTS: &str = r#"{ "is_results_table": false, "orientation": "methods_in_rows", "header_rows": 1,
  "label_col": 0, "settings": [], "columns": [], "rows": [], "review": [] }"#;

const GOLD: &str = r#"
paper = "2401.01234v3"
negative_tables = ["S4.T2"]

[[table]]
id = "S4.T1"
header_rows = 2
settings = { protocol = "filtered" }
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

fn registry() -> Registry {
    let root = Path::new(CONFIG);
    let n = Normalizer::from_toml_str(
        "n",
        &fs::read_to_string(root.join("normalization.toml")).unwrap(),
    )
    .unwrap();
    let kgc = fs::read_to_string(root.join("registry/kgc.toml")).unwrap();
    Registry::build(n, &[("kgc.toml", &kgc)]).unwrap()
}

struct Env {
    dir: tempfile::TempDir,
}

impl Env {
    /// A cache with the fixture paper, a gold spec for it, and (optionally)
    /// recorded answers for every table.
    fn new(record: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arxiv = dir.path().join("cache/arxiv");
        fs::create_dir_all(&arxiv).unwrap();
        fs::copy(FIXTURE, arxiv.join("2401.01234v3.html")).unwrap();
        fs::create_dir_all(dir.path().join("gold/demo")).unwrap();
        fs::write(dir.path().join("gold/demo/2401.01234v3.toml"), GOLD).unwrap();
        let env = Self { dir };
        if record {
            env.record(|table| if table == "S4.T1" { S4_T1 } else { NOT_RESULTS });
        }
        env
    }

    fn path(&self, p: &str) -> PathBuf {
        self.dir.path().join(p)
    }

    fn record(&self, answer: impl Fn(&str) -> &'static str) {
        let html = fs::read_to_string(FIXTURE).unwrap();
        let doc = sota_ingest::arxiv_html::parse(&html, "2401.01234", 3);
        let reg = registry();
        let rec = self.path("recordings");
        fs::create_dir_all(&rec).unwrap();
        for table in &doc.tables {
            let key = recording_key(MODEL, &request_for(&doc, table, &reg));
            let body = serde_json::json!({
                "key": key, "model": MODEL, "content": answer(&table.id),
                "usage": { "input_tokens": 100, "output_tokens": 20, "reasoning_tokens": 5 }
            });
            fs::write(rec.join(format!("{key}.json")), body.to_string()).unwrap();
        }
    }

    fn sota(&self, args: &[&str]) -> Output {
        self.sota_with(args, &[])
    }

    fn sota_with(&self, args: &[&str], vars: &[(&str, &str)]) -> Output {
        let mut all: Vec<String> = vec![
            "--config".into(),
            CONFIG.into(),
            "--cache".into(),
            s(&self.path("cache")),
        ];
        all.extend(args.iter().map(|a| a.to_string()));
        Command::new(env!("CARGO_BIN_EXE_sota"))
            .args(&all)
            .current_dir(self.dir.path()) // no .env here, ever
            .env_remove("AZURE_OPENAI_ENDPOINT")
            .env_remove("AZURE_OPENAI_DEPLOYMENT")
            .env_remove("AZURE_OPENAI_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env_remove("NEO4J_HTTP_URL")
            .envs(vars.iter().copied())
            .output()
            .expect("failed to run sota")
    }

    fn extract(&self, extra: &[&str]) -> Output {
        let rec = s(&self.path("recordings"));
        let out = s(&self.path("out"));
        let mut args = vec![
            "extract",
            "--paper",
            "2401.01234v3",
            "--model",
            MODEL,
            "--recordings",
            &rec,
            "--out",
            &out,
        ];
        args.extend_from_slice(extra);
        self.sota(&args)
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
fn extract_replays_recorded_answers_and_writes_verified_results() {
    let env = Env::new(true);
    let o = env.extract(&[]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(
        text.contains("S4.T1") && text.contains("7 results"),
        "{text}"
    );
    assert!(text.contains("0 calls"), "{text}");

    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(env.path("out/2401.01234v3.json")).unwrap())
            .unwrap();
    assert_eq!(saved["results"].as_array().unwrap().len(), 7);
    assert_eq!(saved["model"], MODEL);
}

#[test]
fn without_recordings_extract_never_calls_out_and_says_how_to() {
    let env = Env::new(false);
    let o = env.extract(&[]);
    assert!(!o.status.success());
    assert!(out(&o).contains("--live"), "{}{}", out(&o), err(&o));
}

#[test]
fn live_mode_without_credentials_names_what_is_missing() {
    let env = Env::new(false);
    let o = env.extract(&["--live"]);
    assert!(!o.status.success());
    assert!(
        err(&o).contains("AZURE_OPENAI_ENDPOINT"),
        "{}{}",
        out(&o),
        err(&o)
    );
}

#[test]
fn eval_scores_an_extraction_against_the_gold_set() {
    let env = Env::new(true);
    let gold = s(&env.path("gold"));
    assert!(
        env.sota(&["gold", "expand", "--dir", &gold])
            .status
            .success()
    );
    assert!(env.extract(&[]).status.success());

    let o = env.sota(&[
        "eval",
        "extract",
        "--dir",
        &gold,
        "--out",
        &s(&env.path("out")),
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(text.contains("2401.01234v3"), "{text}");
    assert!(
        text.contains("precision 1.000") && text.contains("recall 1.000"),
        "{text}"
    );
    assert!(text.contains("0 unscored"), "{text}");
    assert!(env.path("out/eval.json").is_file());
}

#[test]
fn eval_reports_mistakes() {
    let env = Env::new(false);
    // The model swaps the roles of the two rows.
    let wrong = S4_T1
        .replace(r#""role": "baseline""#, r#""role": "TMP""#)
        .replace(r#""role": "proposed""#, r#""role": "baseline""#)
        .replace(r#""role": "TMP""#, r#""role": "proposed""#);
    let wrong: &'static str = Box::leak(wrong.into_boxed_str());
    env.record(move |table| if table == "S4.T1" { wrong } else { NOT_RESULTS });
    let gold = s(&env.path("gold"));
    assert!(
        env.sota(&["gold", "expand", "--dir", &gold])
            .status
            .success()
    );
    assert!(env.extract(&[]).status.success());

    let o = env.sota(&[
        "eval",
        "extract",
        "--dir",
        &gold,
        "--out",
        &s(&env.path("out")),
    ]);
    let text = out(&o);
    // Every result carries the wrong role: none counts.
    assert!(
        text.contains("precision 0.000") && text.contains("recall 0.000"),
        "{text}"
    );
    assert!(
        text.contains("role: gold Baseline, extracted Proposed"),
        "{text}"
    );
}

#[test]
fn dry_run_estimates_the_input_without_calling_or_writing_anything() {
    let env = Env::new(false);
    let o = env.extract(&["--dry-run"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(
        text.contains("tables") && text.contains("input tokens"),
        "{text}"
    );
    assert!(!env.path("out").exists(), "a dry run writes nothing");
}

#[test]
fn leaderboard_ranks_verified_results_with_their_sources() {
    let env = Env::new(true);
    assert!(env.extract(&[]).status.success());
    let out_dir = s(&env.path("out"));
    let o = env.sota(&[
        "leaderboard",
        "FB15K-237",
        "MRR",
        "--source",
        "files",
        "--from",
        &out_dir,
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(
        text.contains("FB15k-237") && text.contains("MRR") && text.contains("higher is better"),
        "{text}"
    );
    assert!(text.contains("protocol=filtered"), "{text}");
    // "Ours (large)" matches no known method: not ranked, but listed with its source.
    assert!(text.contains("| 1 | TransE |"), "{text}");
    assert!(!text.contains("| Ours (large)"), "{text}");
    let not_ranked = text.find("Not ranked").expect(&text);
    assert!(text[not_ranked..].contains("Ours (large)"), "{text}");

    // --all-methods ranks it too, marked as unconfirmed: 0.338 above TransE's 0.294.
    let o = env.sota(&[
        "leaderboard",
        "fb15k-237",
        "mrr",
        "--source",
        "files",
        "--from",
        &out_dir,
        "--all-methods",
    ]);
    let all = out(&o);
    assert!(all.contains("| 1 | Ours (large) ? |"), "{all}");
    assert!(all.contains("| 2 | TransE |"), "{all}");
    assert!(
        text.contains("https://arxiv.org/html/2401.01234v3#S4.T1"),
        "{text}"
    );
}

#[test]
fn leaderboard_filters_by_setting_and_rejects_unknown_names() {
    let env = Env::new(true);
    assert!(env.extract(&[]).status.success());
    let out_dir = s(&env.path("out"));
    let o = env.sota(&[
        "leaderboard",
        "fb15k-237",
        "mrr",
        "--source",
        "files",
        "--from",
        &out_dir,
        "--setting",
        "protocol=raw",
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("no comparable results"), "{}", out(&o));

    let o = env.sota(&[
        "leaderboard",
        "fb15k-238",
        "mrr",
        "--source",
        "files",
        "--from",
        &out_dir,
    ]);
    assert!(!o.status.success());
    assert!(err(&o).contains("fb15k-238"), "{}", err(&o));
    let o = env.sota(&[
        "leaderboard",
        "fb15k-237",
        "mrr",
        "--source",
        "files",
        "--from",
        &out_dir,
        "--setting",
        "protocol=filtred",
    ]);
    assert!(!o.status.success());
    assert!(err(&o).contains("filtred"), "{}", err(&o));
}

#[test]
fn the_gold_leaderboard_matches_a_perfect_extraction() {
    let env = Env::new(true);
    let gold = s(&env.path("gold"));
    assert!(
        env.sota(&["gold", "expand", "--dir", &gold])
            .status
            .success()
    );
    assert!(env.extract(&[]).status.success());
    let board = |source: &str, from: &str| {
        let o = env.sota(&[
            "leaderboard",
            "fb15k-237",
            "mrr",
            "--source",
            source,
            "--from",
            from,
        ]);
        assert!(o.status.success(), "{}{}", out(&o), err(&o));
        out(&o)
    };
    let from_gold = board("gold", &gold);
    assert!(from_gold.contains("| 1 | TransE |"), "{from_gold}");
    assert_eq!(from_gold, board("files", &s(&env.path("out"))));
}

#[test]
fn the_graph_source_says_how_to_start_neo4j_when_it_is_down() {
    let env = Env::new(false);
    let o = env.sota_with(
        &["leaderboard", "fb15k-237", "mrr"],
        &[("NEO4J_HTTP_URL", "http://127.0.0.1:9")],
    );
    assert!(!o.status.success());
    let e = err(&o);
    assert!(
        e.contains("cannot reach Neo4j") && e.contains("docker compose up -d"),
        "{e}"
    );
}
