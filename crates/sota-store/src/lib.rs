//! The knowledge graph, in Neo4j, over its HTTP query API.
//!
//! Plain blocking HTTP (`POST /db/<db>/query/v2`) keeps the CLI free of an
//! async runtime. The schema comes from `config/ontology.toml`: one
//! uniqueness constraint per node key. Loading is idempotent (MERGE on keys),
//! so loading the same results twice changes nothing.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};
use sota_core::{Ontology, Report, Role};

/// Node labels and relationships this writer creates. A test checks that the
/// ontology declares every one, with these ends.
pub const NODE_LABELS: [&str; 5] = ["Paper", "Method", "Dataset", "Metric", "Result"];
pub const RELATIONSHIPS: [(&str, &str, &str); 5] = [
    ("Paper", "PROPOSES", "Method"),
    ("Paper", "REPORTS", "Result"),
    ("Result", "FOR_METHOD", "Method"),
    ("Result", "ON_DATASET", "Dataset"),
    ("Result", "MEASURED_BY", "Metric"),
];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("cannot reach Neo4j at {url}: {message} (is `docker compose up -d` running?)")]
    Transport { url: String, message: String },
    #[error("Neo4j {status} {code}: {message}")]
    Neo4j {
        status: u16,
        code: String,
        message: String,
    },
    #[error("unexpected Neo4j response: {0}")]
    BadResponse(String),
}

pub struct Neo4j {
    url: String,
    database: String,
    auth: String,
    agent: ureq::Agent,
}

/// `Basic base64(user:password)`.
pub fn basic_auth(user: &str, password: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = format!("{user}:{password}").into_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    format!("Basic {out}")
}

impl Neo4j {
    pub fn new(url: &str, database: &str, user: &str, password: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        Self {
            url: url.trim().trim_end_matches('/').to_owned(),
            database: database.to_owned(),
            auth: basic_auth(user, password),
            agent: config.into(),
        }
    }

    /// `NEO4J_HTTP_URL` (default http://localhost:7474), `NEO4J_USERNAME`
    /// (neo4j), `NEO4J_PASSWORD` (the local container's default),
    /// `NEO4J_DATABASE` (neo4j).
    pub fn from_vars(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |name: &str, default: &str| {
            lookup(name)
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| default.to_owned())
        };
        Self::new(
            &get("NEO4J_HTTP_URL", "http://localhost:7474"),
            &get("NEO4J_DATABASE", "neo4j"),
            &get("NEO4J_USERNAME", "neo4j"),
            &get("NEO4J_PASSWORD", "sota-graph-local"),
        )
    }

    /// Runs one statement; returns its rows.
    pub fn run(&self, statement: &str, parameters: Value) -> Result<Vec<Vec<Value>>, StoreError> {
        let url = format!("{}/db/{}/query/v2", self.url, self.database);
        let body = json!({ "statement": statement, "parameters": parameters }).to_string();
        let transport = |e: ureq::Error| StoreError::Transport {
            url: self.url.clone(),
            message: e.to_string(),
        };
        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth)
            .content_type("application/json")
            .send(&body)
            .map_err(transport)?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().map_err(transport)?;
        let json: Value =
            serde_json::from_str(&text).map_err(|e| StoreError::BadResponse(e.to_string()))?;
        if let Some(error) = json.get("errors").and_then(|e| e.get(0)) {
            return Err(StoreError::Neo4j {
                status,
                code: error["code"].as_str().unwrap_or_default().to_owned(),
                message: error["message"].as_str().unwrap_or_default().to_owned(),
            });
        }
        if !(200..300).contains(&status) {
            return Err(StoreError::BadResponse(format!(
                "HTTP {status}: {}",
                text.chars().take(300).collect::<String>()
            )));
        }
        let rows = json["data"]["values"]
            .as_array()
            .ok_or_else(|| StoreError::BadResponse("no data.values".to_owned()))?
            .iter()
            .map(|row| row.as_array().cloned().unwrap_or_default())
            .collect();
        Ok(rows)
    }
}

/// One uniqueness constraint per node key, from the ontology. Labels and
/// property names are safe to interpolate: the ontology only accepts plain
/// identifiers.
pub fn constraints(ontology: &Ontology) -> Vec<String> {
    ontology
        .nodes
        .iter()
        .map(|n| {
            format!(
                "CREATE CONSTRAINT {}_{} IF NOT EXISTS FOR (n:{}) REQUIRE n.{} IS UNIQUE",
                n.label.to_lowercase(),
                n.key,
                n.label,
                n.key
            )
        })
        .collect()
}

pub fn encode_settings(settings: &BTreeMap<String, String>) -> Vec<String> {
    settings.iter().map(|(k, v)| format!("{k}={v}")).collect()
}

pub fn decode_settings(encoded: &[String]) -> BTreeMap<String, String> {
    encoded
        .iter()
        .filter_map(|s| s.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::Proposed => "proposed",
        Role::Baseline => "baseline",
        Role::Variant => "variant",
    }
}

fn parse_role(s: &str) -> Role {
    match s {
        "proposed" => Role::Proposed,
        "variant" => Role::Variant,
        _ => Role::Baseline,
    }
}

/// Everything one MERGE needs for a result and its neighbours.
pub fn result_row(r: &Report, title: &str) -> Value {
    let paper_id = r
        .paper
        .rsplit_once('v')
        .map_or(r.paper.as_str(), |(id, _)| id);
    json!({
        "paper_id": paper_id,
        "paper_version": r.paper,
        "title": title,
        "id": r.locator,
        "method_id": r.method_id,
        "method_name": r.method_name,
        "method_known": r.method_known,
        "role": role_str(r.role),
        "dataset": r.dataset,
        "metric": r.metric,
        "split": r.split,
        "settings": encode_settings(&r.settings),
        "value": r.value,
        "raw_text": r.value_text,
        "channel": "arxiv_html",
    })
}

const LOAD: &str = "
UNWIND $rows AS r
MERGE (p:Paper {arxiv_id: r.paper_id})
  SET p.title = r.title,
      p.versions_seen = CASE WHEN r.paper_version IN coalesce(p.versions_seen, [])
                             THEN p.versions_seen
                             ELSE coalesce(p.versions_seen, []) + r.paper_version END
MERGE (m:Method {id: r.method_id})
  SET m.name = r.method_name, m.known = r.method_known
MERGE (d:Dataset {id: r.dataset})
  SET d.name = r.dataset_name
MERGE (x:Metric {id: r.metric})
  SET x.name = r.metric_name, x.direction = r.direction
MERGE (res:Result {id: r.id})
  SET res.value = r.value, res.raw_text = r.raw_text, res.split = r.split,
      res.settings = r.settings, res.role = r.role, res.channel = r.channel,
      res.locator = r.id
WITH p, m, d, x, res
CALL (res) {
  MATCH (res)-[old:FOR_METHOD|ON_DATASET|MEASURED_BY]->()
  DELETE old
}
MERGE (p)-[:REPORTS]->(res)
MERGE (res)-[:FOR_METHOD]->(m)
MERGE (res)-[:ON_DATASET]->(d)
MERGE (res)-[:MEASURED_BY]->(x)
";

/// PROPOSES follows the results: rebuilt, for each loaded paper, from its
/// results whose role is `proposed`.
const PROPOSES: [&str; 2] = [
    "MATCH (p:Paper)-[old:PROPOSES]->() WHERE p.arxiv_id IN $papers DELETE old",
    "MATCH (p:Paper)-[:REPORTS]->(:Result {role: 'proposed'})-[:FOR_METHOD]->(m:Method)
     WHERE p.arxiv_id IN $papers
     MERGE (p)-[:PROPOSES]->(m)",
];

/// A method is only in the graph through its results.
const ORPHAN_METHODS: &str = "MATCH (m:Method) WHERE NOT (m)--() DELETE m";

/// Rows per statement: large enough to be fast, small enough for the default
/// transaction memory.
const BATCH: usize = 500;

/// Applies the ontology's constraints, then MERGEs every report. A reload
/// replaces what a result points to (its method may have a new id once an
/// alias is added), rebuilds PROPOSES for the loaded papers and drops methods
/// left without results. `rows` come from `result_row`, completed with
/// registry names.
pub fn load(db: &Neo4j, ontology: &Ontology, rows: &[Value]) -> Result<usize, StoreError> {
    for c in constraints(ontology) {
        db.run(&c, json!({}))?;
    }
    for batch in rows.chunks(BATCH) {
        db.run(LOAD, json!({ "rows": batch }))?;
    }
    let mut papers: Vec<&str> = rows.iter().filter_map(|r| r["paper_id"].as_str()).collect();
    papers.sort_unstable();
    papers.dedup();
    for statement in PROPOSES {
        db.run(statement, json!({ "papers": papers }))?;
    }
    db.run(ORPHAN_METHODS, json!({}))?;
    Ok(rows.len())
}

/// Every result on `dataset` and `metric`, as reports for the comparison engine.
pub fn fetch_reports(db: &Neo4j, dataset: &str, metric: &str) -> Result<Vec<Report>, StoreError> {
    let rows = db.run(
        "MATCH (p:Paper)-[:REPORTS]->(r:Result)-[:ON_DATASET]->(:Dataset {id: $dataset}),
               (r)-[:MEASURED_BY]->(:Metric {id: $metric}),
               (r)-[:FOR_METHOD]->(m:Method)
         RETURN r.locator, m.id, m.name, m.known, r.role, r.split, r.settings, r.value, r.raw_text
         ORDER BY r.locator",
        json!({ "dataset": dataset, "metric": metric }),
    )?;
    rows.into_iter()
        .map(|row| {
            let s = |i: usize| {
                row.get(i)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let locator = s(0);
            let settings: Vec<String> = row
                .get(6)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            Ok(Report {
                paper: locator.split('#').next().unwrap_or_default().to_owned(),
                method_id: s(1),
                method_name: s(2),
                method_known: row.get(3).and_then(Value::as_bool).unwrap_or(false),
                role: parse_role(&s(4)),
                dataset: dataset.to_owned(),
                metric: metric.to_owned(),
                split: s(5),
                settings: decode_settings(&settings),
                value: row
                    .get(7)
                    .and_then(Value::as_f64)
                    .ok_or_else(|| StoreError::BadResponse(format!("{locator}: no value")))?,
                value_text: s(8),
                locator,
            })
        })
        .collect()
}

/// Node and relationship counts by label and type.
pub fn stats(db: &Neo4j) -> Result<Vec<(String, i64)>, StoreError> {
    let mut out = Vec::new();
    for row in db.run(
        "MATCH (n) UNWIND labels(n) AS label RETURN label, count(*) ORDER BY label",
        json!({}),
    )? {
        out.push((
            row[0].as_str().unwrap_or_default().to_owned(),
            row[1].as_i64().unwrap_or(0),
        ));
    }
    for row in db.run(
        "MATCH ()-[r]->() RETURN type(r), count(*) ORDER BY type(r)",
        json!({}),
    )? {
        out.push((
            row[0].as_str().unwrap_or_default().to_owned(),
            row[1].as_i64().unwrap_or(0),
        ));
    }
    Ok(out)
}
