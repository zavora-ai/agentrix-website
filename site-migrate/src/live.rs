//! Live fetching over HTTP: public pages in, raw HTML out.
//!
//! [`HttpFetch`] implements [`Fetch`](crate::Fetch) with a 15-second timeout, a
//! 5 MB body cap, and redirect-following: big enough for real pages, bounded
//! against anything else. No credentials exist on this path — it only reads what
//! the public web serves.

use std::io::Read as _;
use std::sync::Arc;
use std::time::Duration;

use agentrix_site_defense::{Breaker, BreakerCall};

use crate::crawl::{CrawlError, Fetch};

/// Maximum fetched body, 5 MB.
pub const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

/// Fetches public URLs over HTTP.
pub struct HttpFetch {
    client: reqwest::blocking::Client,
    breaker: Arc<Breaker>,
}

impl HttpFetch {
    /// A fetcher with the standard timeouts and user agent.
    pub fn new() -> Result<Self, CrawlError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("agentrix-site/0.1 (+migration evidence crawl)")
            .build()
            .map_err(|e| CrawlError::Fetch {
                url: String::new(),
                reason: e.to_string(),
            })?;
        Ok(Self {
            client,
            breaker: Arc::new(Breaker::new("http-fetch", 5, Duration::from_secs(60))),
        })
    }

    /// Guards the fetcher with `breaker` instead of the default.
    pub fn with_breaker(mut self, breaker: Arc<Breaker>) -> Self {
        self.breaker = breaker;
        self
    }
}

impl Default for HttpFetch {
    fn default() -> Self {
        Self::new().expect("an HTTP client builds")
    }
}

impl Fetch for HttpFetch {
    fn fetch(&self, url: &str) -> Result<String, CrawlError> {
        let failed = |reason: String| CrawlError::Fetch {
            url: url.to_string(),
            reason,
        };
        let response = self
            .breaker
            .call(
                3,
                Duration::from_millis(500),
                || {
                    self.client
                        .get(url)
                        .send()
                        .and_then(|answer| answer.error_for_status())
                        .map_err(|e| {
                            let retryable = e.is_timeout()
                                || e.is_connect()
                                || e.status().is_some_and(|status| {
                                    status.is_server_error() || status.as_u16() == 429
                                });
                            (e.to_string(), retryable)
                        })
                },
                |(_, retryable): &(String, bool)| *retryable,
            )
            .map_err(|call| match call {
                BreakerCall::Open(open) => failed(open.to_string()),
                BreakerCall::Error((reason, _)) => failed(reason),
            })?;
        let mut body = Vec::new();
        response
            .take((MAX_BODY_BYTES + 1) as u64)
            .read_to_end(&mut body)
            .map_err(|e| failed(e.to_string()))?;
        if body.len() > MAX_BODY_BYTES {
            return Err(failed(format!("body exceeds {MAX_BODY_BYTES} bytes")));
        }
        String::from_utf8(body).map_err(|e| failed(format!("not UTF-8: {e}")))
    }
}
