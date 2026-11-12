//! Offline tests: no test here calls a model or the network.

use std::cell::Cell as Counter;
use std::rc::Rc;

use serde_json::{Value, json};
use sota_core::{Document, Normalizer, Registry, ReviewBy};
use sota_extract::llm::{
    AzureOpenAi, Completion, LlmClient, LlmError, LlmRequest, Recorded, Usage,
    parse_chat_completion, recording_key,
};
use sota_extract::pipeline::extract_paper;
use sota_extract::prompt::{evidence_paragraphs, request_for, schema, user_prompt};

const KGC: &str = include_str!("../../../config/registry/kgc.toml");
const NORMALIZATION: &str = include_str!("../../../config/normalization.toml");
const FIXTURE: &str = include_str!("../../sota-ingest/tests/fixtures/latexml_tables.html");

fn registry() -> Registry {
    let n = Normalizer::from_toml_str("normalization.toml", NORMALIZATION).unwrap();
    Registry::build(n, &[("kgc.toml", KGC)]).unwrap()
}

fn doc() -> Document {
    sota_ingest::arxiv_html::parse(FIXTURE, "2401.01234", 3)
}

// ------------------------------------------------------------ schema

/// Strict structured output requires every object to list all its
/// properties as required and to forbid additional ones.
fn assert_strict(v: &Value, path: &str) {
    if v.get("type") == Some(&json!("object")) {
        assert_eq!(v["additionalProperties"], json!(false), "{path}");
        let props: Vec<&String> = v["properties"].as_object().unwrap().keys().collect();
        let required: Vec<&str> = v["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        for p in &props {
            assert!(required.contains(&p.as_str()), "{path}.{p} not required");
        }
        for (k, child) in v["properties"].as_object().unwrap() {
            assert_strict(child, &format!("{path}.{k}"));
        }
    }
    if let Some(items) = v.get("items") {
        assert_strict(items, &format!("{path}[]"));
    }
}

#[test]
fn the_schema_is_strict_and_closed_over_the_registry() {
    let s = schema(&registry(), &["S4.p1".to_owned()]);
    assert_strict(&s, "$");
    let col = &s["properties"]["columns"]["items"]["properties"];
    let metrics = col["metric"]["enum"].as_array().unwrap();
    assert!(metrics.contains(&json!("mrr")) && metrics.contains(&json!("unknown")));
    let datasets = col["dataset"]["enum"].as_array().unwrap();
    assert!(datasets.contains(&json!("fb15k-237")) && datasets.contains(&Value::Null));
    let claim = &s["properties"]["settings"]["items"]["properties"];
    assert!(
        claim["setting"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("protocol"))
    );
    assert!(
        claim["value"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("filtered"))
    );
}

// ------------------------------------------------------------ prompt

#[test]
fn the_prompt_shows_the_grid_with_coordinates_and_spans() {
    let d = doc();
    let p = user_prompt(&d, d.table("S4.T1").unwrap(), &registry());
    assert!(p.contains("Table 1"), "{p}");
    assert!(p.contains(r#"r0: c0="Method" [rows r0-r1]"#), "{p}");
    assert!(p.contains(r#"c1="FB15k-237" [cols c1-c2]"#), "{p}");
    assert!(p.contains(r#"r3: c0="Ours (large)""#), "{p}");
    assert!(p.contains(r#"c2="0.338\\pm 0.002""#), "{p}");
}

#[test]
fn evidence_paragraphs_are_those_naming_the_table_or_a_non_default_setting() {
    let d = doc();
    let ids: Vec<String> = evidence_paragraphs(&d, d.table("S4.T1").unwrap(), &registry())
        .iter()
        .map(|p| p.id.clone())
        .collect();
    assert!(ids.contains(&"S1.p1".to_owned()), "{ids:?}"); // "reports filtered metrics"
    assert!(ids.contains(&"S4.SS1.p1".to_owned()), "{ids:?}"); // "Table 1 shows the results"
    let p = user_prompt(&d, d.table("S4.T1").unwrap(), &registry());
    assert!(p.contains("[S1.p1]"), "{p}");
}

#[test]
fn the_request_carries_the_schema_and_no_numbers_are_asked_for() {
    let d = doc();
    let req = request_for(&d, d.table("S4.T1").unwrap(), &registry());
    assert_eq!(req.schema_name, "table_mapping");
    assert!(
        req.system.contains("Never output numbers"),
        "{}",
        req.system
    );
}

// ------------------------------------------------------------ Azure OpenAI (no network)

fn azure(endpoint: &str) -> AzureOpenAi {
    AzureOpenAi::new(
        endpoint,
        "gpt-5.4-mini",
        "secret-key",
        Some("medium".into()),
    )
}

#[test]
fn the_azure_url_and_body_are_well_formed() {
    assert_eq!(
        azure("https://x.openai.azure.com/").url(),
        "https://x.openai.azure.com/openai/v1/chat/completions"
    );
    assert_eq!(
        azure("https://x.openai.azure.com").url(),
        "https://x.openai.azure.com/openai/v1/chat/completions"
    );
    let a = azure("https://x.openai.azure.com");
    let req = LlmRequest {
        system: "sys".into(),
        user: "usr".into(),
        schema_name: "table_mapping".into(),
        schema: json!({"type": "object"}),
    };
    let body = a.body(&req);
    assert_eq!(body["model"], "gpt-5.4-mini");
    assert_eq!(
        body["messages"][0],
        json!({"role": "system", "content": "sys"})
    );
    assert_eq!(
        body["messages"][1],
        json!({"role": "user", "content": "usr"})
    );
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    assert_eq!(body["reasoning_effort"], "medium");
    assert!(!body.to_string().contains("secret-key"));
    assert_eq!(a.model(), "gpt-5.4-mini/medium");
}

fn response(finish: &str, content: Value, refusal: Value) -> Value {
    json!({
        "choices": [{ "finish_reason": finish, "message": { "role": "assistant", "content": content, "refusal": refusal } }],
        "usage": { "prompt_tokens": 1200, "completion_tokens": 300, "completion_tokens_details": { "reasoning_tokens": 120 } }
    })
}

#[test]
fn responses_are_parsed_and_failures_are_typed() {
    let ok =
        parse_chat_completion(&response("stop", json!("{\"a\":1}"), Value::Null), "m").unwrap();
    assert_eq!(ok.content, "{\"a\":1}");
    assert_eq!(
        ok.usage,
        Usage {
            input_tokens: 1200,
            output_tokens: 300,
            reasoning_tokens: 120
        }
    );
    assert!(matches!(
        parse_chat_completion(&response("length", json!("{"), Value::Null), "m"),
        Err(LlmError::Truncated)
    ));
    assert!(matches!(
        parse_chat_completion(&response("content_filter", Value::Null, Value::Null), "m"),
        Err(LlmError::Filtered)
    ));
    assert!(matches!(
        parse_chat_completion(&response("stop", Value::Null, json!("no")), "m"),
        Err(LlmError::Refused(_))
    ));
    assert!(matches!(
        parse_chat_completion(&json!({"oops": 1}), "m"),
        Err(LlmError::BadResponse(_))
    ));
}

// ------------------------------------------------------------ record / replay

/// How a fake client answers one request.
type Answer = Box<dyn Fn(&LlmRequest) -> Result<String, LlmError>>;

/// Answers every request with `answer`, counting calls.
struct Fake {
    answer: Answer,
    calls: Rc<Counter<usize>>,
}

impl LlmClient for Fake {
    fn model(&self) -> String {
        "fake/none".into()
    }
    fn complete(&mut self, req: &LlmRequest) -> Result<Completion, LlmError> {
        self.calls.set(self.calls.get() + 1);
        Ok(Completion {
            content: (self.answer)(req)?,
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
                reasoning_tokens: 0,
            },
            model: self.model(),
        })
    }
}

fn fake(
    answer: impl Fn(&LlmRequest) -> Result<String, LlmError> + 'static,
) -> (Fake, Rc<Counter<usize>>) {
    let calls = Rc::new(Counter::new(0));
    (
        Fake {
            answer: Box::new(answer),
            calls: calls.clone(),
        },
        calls,
    )
}

fn req(user: &str) -> LlmRequest {
    LlmRequest {
        system: "s".into(),
        user: user.into(),
        schema_name: "n".into(),
        schema: json!({}),
    }
}

#[test]
fn replay_only_never_calls_anything_and_says_what_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let mut r: Recorded<Fake> = Recorded::replay_only(dir.path(), "fake/none");
    let err = r.complete(&req("u")).unwrap_err();
    assert!(matches!(err, LlmError::NotRecorded { .. }), "{err:?}");
}

#[test]
fn live_mode_records_once_then_replays() {
    let dir = tempfile::tempdir().unwrap();
    let (inner, calls) = fake(|_| Ok("{}".into()));
    let mut r = Recorded::live(inner, dir.path());
    r.complete(&req("u")).unwrap();
    r.complete(&req("u")).unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!((r.calls, r.replayed), (1, 1));
    let file = dir
        .path()
        .join(format!("{}.json", recording_key("fake/none", &req("u"))));
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(saved["content"], "{}");
    assert!(
        saved.get("user").is_none(),
        "recordings hold answers, not paper text"
    );

    // A replay-only client now finds it, without any inner client.
    let mut replay: Recorded<Fake> = Recorded::replay_only(dir.path(), "fake/none");
    assert_eq!(replay.complete(&req("u")).unwrap().content, "{}");
}

#[test]
fn the_key_changes_with_the_model_and_the_request() {
    assert_ne!(
        recording_key("a/medium", &req("u")),
        recording_key("a/high", &req("u"))
    );
    assert_ne!(
        recording_key("a/medium", &req("u")),
        recording_key("a/medium", &req("v"))
    );
}

// ------------------------------------------------------------ pipeline

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

#[test]
fn a_paper_is_extracted_table_by_table() {
    let (mut client, calls) = fake(|r| {
        Ok(if r.user.contains("Table: S4.T1") {
            S4_T1
        } else {
            NOT_RESULTS
        }
        .into())
    });
    let d = doc();
    let x = extract_paper(&d, &registry(), &mut client, None);
    assert_eq!(calls.get(), d.tables.len());
    assert_eq!(x.results.len(), 7, "{:?}", x.review); // 2 rows x 4 cols, one "-"
    assert_eq!(x.paper, "2401.01234v3");
    assert_eq!(x.usage.input_tokens, 10 * d.tables.len() as u64);
    assert!(x.results.iter().all(|r| r.table == "S4.T1"));
}

#[test]
fn a_failed_or_malformed_answer_sends_the_table_to_review() {
    let (mut client, _) = fake(|r| {
        if r.user.contains("Table: S4.T1") {
            Err(LlmError::Truncated)
        } else {
            Ok("not json".into())
        }
    });
    let d = doc();
    let x = extract_paper(&d, &registry(), &mut client, Some("S4.T1"));
    assert!(x.results.is_empty());
    assert_eq!(x.review.len(), 1);
    assert_eq!(
        (x.review[0].row, x.review[0].col, x.review[0].by),
        (None, None, ReviewBy::Check)
    );
    assert!(
        x.review[0].reason.contains("extraction failed"),
        "{}",
        x.review[0].reason
    );

    let x = extract_paper(&d, &registry(), &mut client, Some("S4.T2"));
    assert!(
        x.review[0].reason.contains("not a valid table mapping"),
        "{}",
        x.review[0].reason
    );
}

#[test]
fn an_authentication_error_stops_the_run_at_the_first_table() {
    let (mut client, calls) = fake(|_| {
        Err(LlmError::Http {
            status: 401,
            body: "Access denied".into(),
        })
    });
    let d = doc();
    let x = extract_paper(&d, &registry(), &mut client, None);
    assert_eq!(calls.get(), 1, "no point repeating a 401 for every table");
    assert!(
        x.aborted.as_deref().is_some_and(|a| a.contains("401")),
        "{:?}",
        x.aborted
    );
    assert_eq!(x.tables.len(), 1);
}

#[test]
fn other_failures_do_not_stop_the_run() {
    let (mut client, calls) = fake(|_| Err(LlmError::Truncated));
    let d = doc();
    let x = extract_paper(&d, &registry(), &mut client, None);
    assert_eq!(calls.get(), d.tables.len());
    assert!(x.aborted.is_none());
}

#[test]
fn evidence_can_only_cite_an_offered_paragraph_the_caption_or_a_header() {
    let d = doc();
    let req = request_for(&d, d.table("S4.T1").unwrap(), &registry());
    let evidence = &req.schema["properties"]["settings"]["items"]["properties"]["evidence"]["enum"];
    let ids: Vec<&str> = evidence
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&"caption") && ids.contains(&"header"),
        "{ids:?}"
    );
    assert!(
        ids.contains(&"S1.p1") && ids.contains(&"S4.SS1.p1"),
        "{ids:?}"
    );
    assert!(
        !ids.contains(&"S4.p1"),
        "only paragraphs offered for this table: {ids:?}"
    );
    let column_claims = &req.schema["properties"]["columns"]["items"]["properties"]["settings"]["items"]
        ["properties"]["evidence"]["enum"];
    assert_eq!(column_claims, evidence);
    let dataset_in = &req.schema["properties"]["dataset_in"]["enum"];
    assert_eq!(dataset_in, &json!(["header", "caption"]));
}
