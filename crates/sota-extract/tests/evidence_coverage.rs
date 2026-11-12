//! Opt-in, against the real cached papers (not in the repo):
//! `cargo test -p sota-extract --test evidence_coverage -- --ignored --nocapture`
//!
//! For every gold table, every required setting the gold states must be
//! provable from what the prompt offers: either the caption names the value,
//! or at least one offered paragraph does. A table whose paper never names
//! the value in words (PairRE describes the filtered protocol without the
//! word) is reported apart: no selection can fix it. Costs no model call.

use std::fs;
use std::path::{Path, PathBuf};

use sota_core::{GoldFile, Normalizer, Registry, mentions_value};
use sota_extract::prompt::evidence_paragraphs;

const KGC: &str = include_str!("../../../config/registry/kgc.toml");
const NORMALIZATION: &str = include_str!("../../../config/normalization.toml");

fn registry() -> Registry {
    let n = Normalizer::from_toml_str("normalization.toml", NORMALIZATION).unwrap();
    Registry::build(n, &[("kgc.toml", KGC)]).unwrap()
}

fn specs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            specs(&path, out);
        } else if path.extension().is_some_and(|e| e == "toml") {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "needs the paper cache: cargo run -p sota-cli -- fetch --list papers/kgc.txt"]
fn the_evidence_for_every_required_setting_is_offered() {
    let r = registry();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = Vec::new();
    specs(&root.join("gold"), &mut paths);
    paths.sort();
    let (mut covered, mut unnamed, mut missing) = (0, Vec::new(), Vec::new());
    for path in &paths {
        let gold = GoldFile::from_toml_str(
            &path.display().to_string(),
            &fs::read_to_string(path).unwrap(),
        )
        .unwrap();
        let html = root
            .join("cache/arxiv")
            .join(format!("{}.html", gold.paper));
        let Ok(html) = fs::read_to_string(&html) else {
            continue;
        };
        let (base, version) = gold.paper.rsplit_once('v').unwrap();
        let doc = sota_ingest::arxiv_html::parse(&html, base, version.parse().unwrap());
        for t in &gold.tables {
            let table = doc.table(&t.id).unwrap();
            let offered = evidence_paragraphs(&doc, table, &r);
            for s in r.settings().iter().filter(|s| s.required) {
                let Some(value) = t.settings.get(&s.id) else {
                    continue;
                };
                let names = |text: &str| mentions_value(text, &s.id, value, &r);
                let at = format!("{} {} {}={}", gold.paper, t.id, s.id, value);
                if names(&table.caption) || offered.iter().any(|p| names(&p.text)) {
                    covered += 1;
                } else if doc
                    .sections
                    .iter()
                    .flat_map(|s| &s.paragraphs)
                    .any(|p| names(&p.text))
                {
                    missing.push(at);
                } else {
                    unnamed.push(at);
                }
            }
        }
    }
    println!(
        "evidence offered: {covered}/{} | never named in the paper: {} {unnamed:?} | named but not offered: {} {missing:?}",
        covered + missing.len(),
        unnamed.len(),
        missing.len()
    );
    assert!(
        missing.is_empty(),
        "evidence exists but is not offered: {missing:?}"
    );
}
