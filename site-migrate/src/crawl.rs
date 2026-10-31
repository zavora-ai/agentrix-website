//! The credential-free crawl: public pages in, parsed evidence out.
//!
//! [`Fetch`] is the only network surface, and it carries no auth: a URL in, raw
//! HTML out. [`FileFetch`] serves local files for offline runs and tests; a live
//! HTTP fetcher implements the same trait. Every fetch is logged with its byte
//! count and SHA-256, so the candidate's provenance names what it was built from.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

/// What the crawl can get wrong.
#[derive(Debug, Error)]
pub enum CrawlError {
    /// The fetch failed.
    #[error("fetch {url}: {reason}")]
    Fetch {
        /// The URL.
        url: String,
        /// Why.
        reason: String,
    },
}

/// Fetches one public URL with no credentials: the HTML in, nothing else.
pub trait Fetch: Send + Sync {
    /// The raw HTML of `url`, or why it could not be fetched.
    fn fetch(&self, url: &str) -> Result<String, CrawlError>;
}

/// Serves files from `dir`: the URL's path becomes the file name, so
/// `https://shop.example/beans.html` reads `<dir>/beans.html`.
pub struct FileFetch {
    dir: PathBuf,
}

impl FileFetch {
    /// A fetcher over `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl Fetch for FileFetch {
    fn fetch(&self, url: &str) -> Result<String, CrawlError> {
        let path = url.rsplit("://").next().unwrap_or(url);
        let file = path
            .split('/')
            .next_back()
            .filter(|name| !name.is_empty())
            .unwrap_or("index.html");
        std::fs::read_to_string(self.dir.join(file)).map_err(|e| CrawlError::Fetch {
            url: url.to_string(),
            reason: e.to_string(),
        })
    }
}

/// A product found on a legacy page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Product {
    /// Product name.
    pub name: String,
    /// Price text as written.
    pub price: String,
}

/// One fetched legacy page, parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidencePage {
    /// The public URL it came from.
    pub url: String,
    /// Slug derived from the URL.
    pub slug: String,
    /// The `<title>`.
    pub title: String,
    /// Visible text, tags stripped.
    pub text: String,
    /// Every link target.
    pub links: Vec<String>,
    /// Every `data-product` element.
    pub products: Vec<Product>,
    /// Every `<meta name|property content>` pair.
    pub metadata: HashMap<String, String>,
}

/// One fetch's log line: what was read, how much, and its hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceEntry {
    /// The URL.
    pub url: String,
    /// When it was fetched.
    pub fetched_at: DateTime<Utc>,
    /// Raw bytes.
    pub bytes: usize,
    /// SHA-256 of the raw HTML.
    pub sha256: String,
}

/// The parsed legacy site plus its fetch log: everything the candidate is built
/// from, and everything parity is scored against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// The pages.
    pub pages: Vec<EvidencePage>,
    /// The fetch log, one entry per URL.
    pub log: Vec<EvidenceEntry>,
}

/// Crawls `urls` through `fetch` and parses each page. A failed fetch fails the
/// crawl: silence never scores, so partial evidence never migrates quietly.
pub fn crawl(fetch: &dyn Fetch, urls: &[String]) -> Result<Evidence, CrawlError> {
    let mut pages = Vec::new();
    let mut log = Vec::new();
    for url in urls {
        let html = fetch.fetch(url)?;
        let mut hash = String::new();
        for byte in Sha256::digest(html.as_bytes()) {
            hash.push_str(&format!("{byte:02x}"));
        }
        log.push(EvidenceEntry {
            url: url.clone(),
            fetched_at: Utc::now(),
            bytes: html.len(),
            sha256: hash,
        });
        pages.push(parse(url, &html));
    }
    Ok(Evidence { pages, log })
}

fn slug_of(url: &str) -> String {
    let path = url.rsplit("://").next().unwrap_or(url);
    let last = path.split('/').next_back().unwrap_or("");
    let stem = last.split('.').next().unwrap_or("");
    if stem.is_empty() {
        "home".to_string()
    } else {
        stem.to_string()
    }
}

fn parse(url: &str, html: &str) -> EvidencePage {
    EvidencePage {
        url: url.to_string(),
        slug: slug_of(url),
        title: tag_text(html, "title"),
        text: visible_text(html),
        links: attributes(html, "href"),
        products: products(html),
        metadata: metadata(html),
    }
}

fn tag_text(html: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let Some(start) = html
        .find(&open)
        .and_then(|i| html[i..].find('>').map(|j| i + j + 1))
    else {
        return String::new();
    };
    let Some(end) = html[start..].find(&close).map(|j| start + j) else {
        return String::new();
    };
    html[start..end].trim().to_string()
}

/// Every value of `attr="..."` in the document.
fn attributes(html: &str, attr: &str) -> Vec<String> {
    let needle = format!("{attr}=\"");
    let mut values = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find(&needle) {
        rest = &rest[start + needle.len()..];
        if let Some(end) = rest.find('"') {
            values.push(rest[..end].to_string());
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    values
}

fn products(html: &str) -> Vec<Product> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<") {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        if let (Some(name), Some(price)) =
            (attr_in(tag, "data-product"), attr_in(tag, "data-price"))
        {
            found.push(Product { name, price });
        }
        rest = &rest[end + 1..];
    }
    found
}

fn attr_in(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

fn metadata(html: &str) -> HashMap<String, String> {
    let mut meta = HashMap::new();
    let mut rest = html;
    while let Some(start) = rest.find("<meta") {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        let key = attr_in(tag, "name").or_else(|| attr_in(tag, "property"));
        if let (Some(key), Some(value)) = (key, attr_in(tag, "content")) {
            meta.insert(key, value);
        }
        rest = &rest[end + 1..];
    }
    meta
}

/// Visible text: scripts, styles and tags removed, whitespace collapsed.
fn visible_text(html: &str) -> String {
    let mut text = html.to_string();
    for tag in ["script", "style"] {
        loop {
            let open = format!("<{tag}");
            let close = format!("</{tag}>");
            let Some(start) = text.find(&open) else { break };
            let Some(end) = text[start..].find(&close).map(|i| start + i + close.len()) else {
                break;
            };
            text.replace_range(start..end, " ");
        }
    }
    let mut clean = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                clean.push(' ');
            }
            _ if !in_tag => clean.push(c),
            _ => {}
        }
    }
    clean.split_whitespace().collect::<Vec<_>>().join(" ")
}
