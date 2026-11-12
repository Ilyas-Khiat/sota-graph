use std::collections::VecDeque;
use std::time::Duration;

use sota_core::ArxivId;
use sota_ingest::cache::{Cache, sha256_hex};
use sota_ingest::fetch::{FetchError, FetchOutcome, Fetcher, Http, HttpResponse};

const PAGE: &str = include_str!("fixtures/latexml_tables.html"); // says v3 of 2401.01234

/// Replays canned responses and records every URL asked for.
#[derive(Default)]
struct FakeHttp {
    responses: VecDeque<HttpResponse>,
    requested: Vec<String>,
}

impl FakeHttp {
    fn with(responses: Vec<HttpResponse>) -> Self {
        Self {
            responses: responses.into(),
            requested: Vec::new(),
        }
    }
}

impl Http for FakeHttp {
    fn get(&mut self, url: &str) -> Result<HttpResponse, String> {
        self.requested.push(url.to_owned());
        self.responses
            .pop_front()
            .ok_or_else(|| "no more canned responses".to_owned())
    }
}

fn ok(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        retry_after: None,
        body: body.as_bytes().to_vec(),
    }
}

fn status(code: u16, retry_after: Option<u64>) -> HttpResponse {
    HttpResponse {
        status: code,
        retry_after: retry_after.map(Duration::from_secs),
        body: Vec::new(),
    }
}

fn id(s: &str) -> ArxivId {
    s.parse().unwrap()
}

fn fetcher(responses: Vec<HttpResponse>) -> Fetcher<FakeHttp> {
    let mut f = Fetcher::new(FakeHttp::with(responses), |_| {});
    f.min_interval = Duration::from_secs(15);
    f.backoff = vec![Duration::from_secs(30), Duration::from_secs(60)];
    f
}

fn secs(f: &Fetcher<FakeHttp>) -> Vec<u64> {
    f.sleeps.iter().map(Duration::as_secs).collect()
}

#[test]
fn fetches_the_pinned_version_and_caches_it_with_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path());
    let mut f = fetcher(vec![ok(PAGE)]);

    let out = f.fetch(&id("2401.01234v3"), &cache).unwrap();
    assert!(matches!(out, FetchOutcome::Fetched { .. }), "{out:?}");
    assert_eq!(f.http.requested, ["https://arxiv.org/html/2401.01234v3"]);
    assert_eq!(cache.read_html(&id("2401.01234v3")).unwrap(), PAGE);

    let meta = cache.read_meta(&id("2401.01234v3")).unwrap();
    assert_eq!(meta.url, "https://arxiv.org/html/2401.01234v3");
    assert_eq!(meta.sha256, sha256_hex(PAGE.as_bytes()));
    assert_eq!(
        meta.generator.as_deref(),
        Some("LaTeXML oxide (version 0.7.6)")
    );
}

#[test]
fn a_cached_version_is_never_requested_again() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path());
    let mut f = fetcher(vec![ok(PAGE)]);
    f.fetch(&id("2401.01234v3"), &cache).unwrap();

    let out = f.fetch(&id("2401.01234v3"), &cache).unwrap();
    assert!(matches!(out, FetchOutcome::Cached(_)), "{out:?}");
    assert_eq!(f.http.requested.len(), 1);
    assert!(f.sleeps.is_empty());
    assert_eq!(cache.list().unwrap(), [id("2401.01234v3")]);
}

#[test]
fn an_unpinned_id_is_refused_before_any_request() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = fetcher(vec![ok(PAGE)]);
    let err = f
        .fetch(&id("2401.01234"), &Cache::new(dir.path()))
        .unwrap_err();
    assert!(matches!(err, FetchError::NeedsVersion(_)), "{err:?}");
    assert!(f.http.requested.is_empty());
}

#[test]
fn requests_are_spaced_by_the_minimum_interval() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path());
    let other = PAGE.replace("2401.01234v3", "2401.05678v1");
    let mut f = fetcher(vec![ok(PAGE), ok(&other)]);
    f.fetch(&id("2401.01234v3"), &cache).unwrap();
    assert_eq!(secs(&f), Vec::<u64>::new()); // the first request does not wait
    f.fetch(&id("2401.05678v1"), &cache).unwrap();
    assert_eq!(secs(&f), [15]);
}

#[test]
fn rate_limits_are_retried_honoring_retry_after() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = fetcher(vec![status(429, Some(40)), status(503, None), ok(PAGE)]);
    let out = f
        .fetch(&id("2401.01234v3"), &Cache::new(dir.path()))
        .unwrap();
    assert!(matches!(out, FetchOutcome::Fetched { .. }));
    // Retry-After (40 s) beats the first backoff (30 s); then the second backoff (60 s).
    assert_eq!(secs(&f), [40, 15, 60, 15]);
    assert_eq!(f.http.requested.len(), 3);
}

#[test]
fn gives_up_when_backoff_is_exhausted_and_caches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path());
    let mut f = fetcher(vec![
        status(429, None),
        status(429, None),
        status(429, None),
    ]);
    let err = f.fetch(&id("2401.01234v3"), &cache).unwrap_err();
    assert!(
        matches!(err, FetchError::GaveUp { attempts: 3, .. }),
        "{err:?}"
    );
    assert!(!cache.contains(&id("2401.01234v3")));
}

#[test]
fn not_found_means_no_html_version() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path());
    let mut f = fetcher(vec![status(404, None)]);
    let out = f.fetch(&id("2401.01234v3"), &cache).unwrap();
    assert!(matches!(out, FetchOutcome::NoHtml), "{out:?}");
    assert!(!cache.contains(&id("2401.01234v3")));
}

#[test]
fn a_page_not_made_by_latexml_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path());
    let mut f = fetcher(vec![ok("<html><body>Conversion failed</body></html>")]);
    let err = f.fetch(&id("2401.01234v3"), &cache).unwrap_err();
    assert!(matches!(err, FetchError::NotLatexml { .. }), "{err:?}");
    assert!(!cache.contains(&id("2401.01234v3")));
}

#[test]
fn a_page_claiming_another_version_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::new(dir.path());
    let mut f = fetcher(vec![ok(PAGE)]);
    let err = f.fetch(&id("2401.01234v4"), &cache).unwrap_err();
    assert!(
        matches!(
            err,
            FetchError::VersionMismatch {
                asked: 4,
                served: 3
            }
        ),
        "{err:?}"
    );
    assert!(!cache.contains(&id("2401.01234v4")));
}

#[test]
fn other_statuses_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = fetcher(vec![status(403, None)]);
    let err = f
        .fetch(&id("2401.01234v3"), &Cache::new(dir.path()))
        .unwrap_err();
    assert!(
        matches!(err, FetchError::Status { status: 403, .. }),
        "{err:?}"
    );
}
