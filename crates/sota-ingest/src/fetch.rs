//! Polite downloader for arxiv.org/html.
//!
//! arXiv's robots.txt allows /html but asks for 15 s between requests and
//! forbids indiscriminate downloads. So: explicit pinned ids only, a fixed
//! pause before every request but the first, backoff on 429/5xx honoring
//! Retry-After, and a cache that makes every version a one-time download.

use std::path::PathBuf;
use std::time::Duration;

use sota_core::ArxivId;

use crate::arxiv_html::{generator, stated_version};
use crate::cache::Cache;

pub const USER_AGENT: &str = concat!(
    "sota-graph/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Ilyas-Khiat/sota-graph)"
);

#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub retry_after: Option<Duration>,
    pub body: Vec<u8>,
}

/// The one network operation the fetcher needs; tests replace it with a fake.
pub trait Http {
    /// `Err` only for transport failures; HTTP error statuses are `Ok`.
    fn get(&mut self, url: &str) -> Result<HttpResponse, String>;
}

#[derive(Debug)]
pub enum FetchOutcome {
    Cached(PathBuf),
    Fetched {
        path: PathBuf,
        bytes: usize,
    },
    /// arXiv has no HTML for this version (404). The PDF channel is the fallback.
    NoHtml,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("{0}: pin a version (e.g. {0}v1); unpinned ids are never fetched")]
    NeedsVersion(String),
    #[error("{url}: HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("{url}: still failing after {attempts} attempts (last: {last})")]
    GaveUp {
        url: String,
        attempts: usize,
        last: String,
    },
    #[error("{url}: not a LaTeXML page")]
    NotLatexml { url: String },
    #[error("asked for v{asked}, but the page says it is v{served}")]
    VersionMismatch { asked: u32, served: u32 },
    #[error("cache: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Fetcher<H: Http> {
    pub http: H,
    sleep: Box<dyn FnMut(Duration)>,
    /// Pause before every request except the first.
    pub min_interval: Duration,
    /// Extra waits after a 429, a 5xx or a transport error; one retry per entry.
    pub backoff: Vec<Duration>,
    /// Every pause requested, in order (for reporting and tests).
    pub sleeps: Vec<Duration>,
    requested_before: bool,
}

impl<H: Http> Fetcher<H> {
    pub fn new(http: H, sleep: impl FnMut(Duration) + 'static) -> Self {
        Self {
            http,
            sleep: Box::new(sleep),
            min_interval: Duration::from_secs(15),
            backoff: [30, 60, 120].map(Duration::from_secs).to_vec(),
            sleeps: Vec::new(),
            requested_before: false,
        }
    }

    pub fn fetch(&mut self, id: &ArxivId, cache: &Cache) -> Result<FetchOutcome, FetchError> {
        let Some(version) = id.version else {
            return Err(FetchError::NeedsVersion(id.to_string()));
        };
        if cache.contains(id) {
            return Ok(FetchOutcome::Cached(cache.html_path(id)));
        }

        let url = format!("https://arxiv.org/html/{id}");
        let mut attempt = 0;
        let response = loop {
            if self.requested_before {
                self.pause(self.min_interval);
            }
            self.requested_before = true;
            attempt += 1;

            let (retry_after, last) = match self.http.get(&url) {
                Ok(r) if r.status == 429 || r.status >= 500 => {
                    (r.retry_after, format!("HTTP {}", r.status))
                }
                Ok(r) => break r,
                Err(transport) => (None, transport),
            };
            let Some(&backoff) = self.backoff.get(attempt - 1) else {
                return Err(FetchError::GaveUp {
                    url,
                    attempts: attempt,
                    last,
                });
            };
            self.pause(retry_after.map_or(backoff, |r| r.max(backoff)));
        };

        match response.status {
            200 => {}
            404 => return Ok(FetchOutcome::NoHtml),
            status => return Err(FetchError::Status { url, status }),
        }
        let text = String::from_utf8_lossy(&response.body);
        let Some(generator) = generator(&text) else {
            return Err(FetchError::NotLatexml { url });
        };
        if let Some(served) = stated_version(&text, &id.id)
            && served != version
        {
            return Err(FetchError::VersionMismatch {
                asked: version,
                served,
            });
        }
        let path = cache.put(id, &url, &response.body, Some(generator))?;
        Ok(FetchOutcome::Fetched {
            path,
            bytes: response.body.len(),
        })
    }

    fn pause(&mut self, how_long: Duration) {
        self.sleeps.push(how_long);
        (self.sleep)(how_long);
    }
}

/// Real HTTP over rustls.
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for UreqHttp {
    fn get(&mut self, url: &str) -> Result<HttpResponse, String> {
        let mut response = self.agent.get(url).call().map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let body = response
            .body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| e.to_string())?;
        Ok(HttpResponse {
            status,
            retry_after,
            body,
        })
    }
}
