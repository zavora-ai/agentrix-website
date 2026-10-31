//! Live fetching over HTTP: public pages in, raw HTML out.
//!
//! [`HttpFetch`] implements [`Fetch`](crate::Fetch) with a 15-second timeout, a
//! 5 MB body cap, and redirect-following: big enough for real pages, bounded
//! against anything else. No credentials exist on this path — it only reads what
//! the public web serves.

use std::io::Read as _;
use std::time::Duration;

use crate::crawl::{CrawlError, Fetch};

/// Maximum fetched body, 5 MB.
pub const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

/// Fetches public URLs over HTTP.
pub struct HttpFetch {
    client: reqwest::blocking::Client,
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
        Ok(Self { client })
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
            .client
            .get(url)
            .send()
            .map_err(|e| failed(e.to_string()))?
            .error_for_status()
            .map_err(|e| failed(e.to_string()))?;
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
