//! Model access behind one interface.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// One structured-output request: instructions, input, and the JSON schema
/// the answer must follow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmRequest {
    pub system: String,
    pub user: String,
    pub schema_name: String,
    pub schema: Value,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Part of `output_tokens` spent reasoning.
    pub reasoning_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Completion {
    /// The answer: a JSON document following the request's schema.
    pub content: String,
    pub usage: Usage,
    pub model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("the model refused: {0}")]
    Refused(String),
    #[error("the answer was cut off (output token limit)")]
    Truncated,
    #[error("the answer was blocked by a content filter")]
    Filtered,
    #[error("unexpected response: {0}")]
    BadResponse(String),
    #[error("no recorded answer {key}; run with --live to call the model")]
    NotRecorded { key: String },
    #[error("recordings: {0}")]
    Io(#[from] io::Error),
}

pub trait LlmClient {
    /// Model and settings that change its answers, e.g. "gpt-5.4-mini/medium".
    /// Part of every recording key.
    fn model(&self) -> String;
    fn complete(&mut self, req: &LlmRequest) -> Result<Completion, LlmError>;
}

// ------------------------------------------------------------------ Azure

/// Azure OpenAI, v1 chat completions with strict JSON-schema output.
pub struct AzureOpenAi {
    endpoint: String,
    deployment: String,
    api_key: String,
    reasoning_effort: Option<String>,
    agent: ureq::Agent,
    /// Waits after a 429 or 5xx; one retry per entry.
    pub backoff: Vec<Duration>,
}

impl AzureOpenAi {
    pub fn new(
        endpoint: &str,
        deployment: &str,
        api_key: &str,
        reasoning_effort: Option<String>,
    ) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(300)))
            .build();
        Self {
            endpoint: endpoint.trim().trim_end_matches('/').to_owned(),
            deployment: deployment.trim().to_owned(),
            api_key: api_key.trim().to_owned(),
            reasoning_effort,
            agent: config.into(),
            backoff: [5, 20, 60].map(Duration::from_secs).to_vec(),
        }
    }

    /// From the process environment; see `from_vars`.
    pub fn from_env() -> Result<Self, String> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// From `AZURE_OPENAI_ENDPOINT`, `AZURE_OPENAI_DEPLOYMENT`, and
    /// `AZURE_OPENAI_API_KEY` or `OPENAI_API_KEY`; `SOTA_REASONING_EFFORT`
    /// (default "medium") is optional. `lookup` lets a caller add a `.env`
    /// file without touching the process environment.
    pub fn from_vars(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let var = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| lookup(n).filter(|v| !v.trim().is_empty()))
                .ok_or_else(|| format!("{} is not set (see .env.example)", names.join(" or ")))
        };
        let endpoint = var(&["AZURE_OPENAI_ENDPOINT"])?;
        let deployment = var(&["AZURE_OPENAI_DEPLOYMENT"])?;
        let key = var(&["AZURE_OPENAI_API_KEY", "OPENAI_API_KEY"])?;
        let effort = lookup("SOTA_REASONING_EFFORT").unwrap_or_else(|| "medium".to_owned());
        Ok(Self::new(&endpoint, &deployment, &key, Some(effort)))
    }

    pub fn url(&self) -> String {
        format!("{}/openai/v1/chat/completions", self.endpoint)
    }

    /// The request body. The API key travels in a header, never in here.
    pub fn body(&self, req: &LlmRequest) -> Value {
        let mut body = json!({
            "model": self.deployment,
            "messages": [
                { "role": "system", "content": req.system },
                { "role": "user", "content": req.user },
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": { "name": req.schema_name, "strict": true, "schema": req.schema },
            },
            "max_completion_tokens": 16000,
        });
        if let Some(effort) = &self.reasoning_effort {
            body["reasoning_effort"] = json!(effort);
        }
        body
    }
}

impl LlmClient for AzureOpenAi {
    fn model(&self) -> String {
        format!(
            "{}/{}",
            self.deployment,
            self.reasoning_effort.as_deref().unwrap_or("default")
        )
    }

    fn complete(&mut self, req: &LlmRequest) -> Result<Completion, LlmError> {
        let body = self.body(req).to_string();
        let mut attempt = 0;
        loop {
            let mut response = self
                .agent
                .post(&self.url())
                .header("api-key", &self.api_key)
                .content_type("application/json")
                .send(&body)
                .map_err(|e| LlmError::Transport(e.to_string()))?;
            let status = response.status().as_u16();
            let text = response
                .body_mut()
                .with_config()
                .limit(32 * 1024 * 1024)
                .read_to_string()
                .map_err(|e| LlmError::Transport(e.to_string()))?;
            if (status == 429 || status >= 500) && attempt < self.backoff.len() {
                std::thread::sleep(self.backoff[attempt]);
                attempt += 1;
                continue;
            }
            if !(200..300).contains(&status) {
                let body: String = text.chars().take(500).collect();
                return Err(LlmError::Http { status, body });
            }
            let json: Value =
                serde_json::from_str(&text).map_err(|e| LlmError::BadResponse(e.to_string()))?;
            return parse_chat_completion(&json, &self.model());
        }
    }
}

/// The answer, or why there is none: cut off, filtered, refused, malformed.
pub fn parse_chat_completion(json: &Value, model: &str) -> Result<Completion, LlmError> {
    let choice = json
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| LlmError::BadResponse("no choices".to_owned()))?;
    match choice.get("finish_reason").and_then(Value::as_str) {
        Some("length") => return Err(LlmError::Truncated),
        Some("content_filter") => return Err(LlmError::Filtered),
        _ => {}
    }
    let message = &choice["message"];
    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) {
        return Err(LlmError::Refused(refusal.to_owned()));
    }
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| LlmError::BadResponse("no message content".to_owned()))?;
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    let usage = &json["usage"];
    Ok(Completion {
        content: content.to_owned(),
        usage: Usage {
            input_tokens: n(&usage["prompt_tokens"]),
            output_tokens: n(&usage["completion_tokens"]),
            reasoning_tokens: n(&usage["completion_tokens_details"]["reasoning_tokens"]),
        },
        model: model.to_owned(),
    })
}

// ------------------------------------------------------------------ record / replay

/// sha256 of the model and the whole request: a different prompt, schema or
/// model means a different answer, so a different recording.
pub fn recording_key(model: &str, req: &LlmRequest) -> String {
    let canonical = json!({
        "model": model,
        "system": req.system,
        "user": req.user,
        "schema_name": req.schema_name,
        "schema": req.schema,
    });
    Sha256::digest(canonical.to_string().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What is stored: the answer and its cost, not the request (which holds the
/// paper's text).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Recording {
    key: String,
    model: String,
    content: String,
    usage: Usage,
}

/// Answers from recordings when it has them; otherwise calls the inner client
/// (live mode) and records the answer, or fails (replay-only mode).
pub struct Recorded<C: LlmClient> {
    inner: Option<C>,
    model: String,
    dir: PathBuf,
    /// Requests sent to the inner client.
    pub calls: usize,
    /// Requests answered from recordings.
    pub replayed: usize,
}

impl<C: LlmClient> Recorded<C> {
    pub fn live(inner: C, dir: impl Into<PathBuf>) -> Self {
        Self {
            model: inner.model(),
            inner: Some(inner),
            dir: dir.into(),
            calls: 0,
            replayed: 0,
        }
    }

    /// No network, ever: a request without a recording is an error.
    pub fn replay_only(dir: impl AsRef<Path>, model: &str) -> Self {
        Self {
            inner: None,
            model: model.to_owned(),
            dir: dir.as_ref().to_path_buf(),
            calls: 0,
            replayed: 0,
        }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }
}

impl<C: LlmClient> LlmClient for Recorded<C> {
    fn model(&self) -> String {
        self.model.clone()
    }

    fn complete(&mut self, req: &LlmRequest) -> Result<Completion, LlmError> {
        let key = recording_key(&self.model, req);
        let path = self.path(&key);
        if path.is_file() {
            let r: Recording = serde_json::from_str(&fs::read_to_string(&path)?)
                .map_err(|e| LlmError::BadResponse(format!("{}: {e}", path.display())))?;
            self.replayed += 1;
            return Ok(Completion {
                content: r.content,
                usage: r.usage,
                model: r.model,
            });
        }
        let Some(inner) = self.inner.as_mut() else {
            return Err(LlmError::NotRecorded { key });
        };
        let completion = inner.complete(req)?;
        self.calls += 1;
        fs::create_dir_all(&self.dir)?;
        let recording = Recording {
            key,
            model: completion.model.clone(),
            content: completion.content.clone(),
            usage: completion.usage,
        };
        let text = serde_json::to_string_pretty(&recording).map_err(io::Error::other)?;
        fs::write(path, text + "\n")?;
        Ok(completion)
    }
}
