//! The per-site corpus: chunks keyed by `(site, chunk)`, retrieved by site only.
//!
//! Ranking is keyword overlap — honest, model-free, and good enough for the
//! fixture corpora. Vector ranking plugs in behind [`retrieve`]'s shape later;
//! the scoping guarantee lives in the SQL `WHERE site = ?`, not in the ranker.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What the corpus can get wrong.
#[derive(Debug, Error)]
pub enum CorpusError {
    /// The database failed.
    #[error("corpus: {0}")]
    Db(#[from] sqlx::Error),
}

/// One retrievable chunk: its site, its document, its source, its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    /// The owning site.
    pub site: String,
    /// The document within the site, such as a page slug.
    pub doc: String,
    /// Stable within the site.
    pub chunk_id: String,
    /// The evidence source, carried into citations (Requirement 4.5).
    pub source_uri: String,
    /// The text.
    pub text: String,
}

/// A chunk with its query score, 0.0 to 1.0.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredChunk {
    /// The chunk.
    pub chunk: Chunk,
    /// Share of distinct query tokens the chunk contains.
    pub score: f64,
}

/// The chunks of every site, in one SQLite database, always read per site.
pub struct Corpus {
    pool: sqlx::sqlite::SqlitePool,
}

impl Corpus {
    /// Opens (creating and migrating) the corpus at `url`.
    pub async fn open(url: &str) -> Result<Self, CorpusError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS chunks (
                site TEXT NOT NULL,
                doc TEXT NOT NULL,
                chunk_id TEXT NOT NULL,
                source_uri TEXT NOT NULL,
                text TEXT NOT NULL,
                PRIMARY KEY (site, chunk_id)
            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    /// Stores (or replaces) `chunk`. The `(site, chunk_id)` key means two sites
    /// never overwrite each other's chunks.
    pub async fn put(&self, chunk: &Chunk) -> Result<(), CorpusError> {
        sqlx::query(
            "INSERT INTO chunks (site, doc, chunk_id, source_uri, text)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(site, chunk_id) DO UPDATE SET doc = ?, source_uri = ?, text = ?",
        )
        .bind(&chunk.site)
        .bind(&chunk.doc)
        .bind(&chunk.chunk_id)
        .bind(&chunk.source_uri)
        .bind(&chunk.text)
        .bind(&chunk.doc)
        .bind(&chunk.source_uri)
        .bind(&chunk.text)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Every chunk of `site` — the only read path the ranker gets, so scoping is
    /// structural: callers cannot ask for another site's chunks through this API.
    pub async fn site_chunks(&self, site: &str) -> Result<Vec<Chunk>, CorpusError> {
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT site, doc, chunk_id, source_uri, text FROM chunks WHERE site = ?",
        )
        .bind(site)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(site, doc, chunk_id, source_uri, text)| Chunk {
                site,
                doc,
                chunk_id,
                source_uri,
                text,
            })
            .collect())
    }
}

/// The top `k` chunks of `site` for `query`, best first. Only `site`'s chunks are
/// read, so only `site`'s chunks can score.
pub async fn retrieve(
    corpus: &Corpus,
    site: &str,
    query: &str,
    k: usize,
) -> Result<Vec<ScoredChunk>, CorpusError> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let mut scored: Vec<ScoredChunk> = corpus
        .site_chunks(site)
        .await?
        .into_iter()
        .map(|chunk| {
            let words = tokenize(&chunk.text);
            let hits = tokens.iter().filter(|tok| words.contains(tok)).count();
            let score = hits as f64 / tokens.len() as f64;
            ScoredChunk { chunk, score }
        })
        .filter(|scored| scored.score > 0.0)
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(k);
    Ok(scored)
}

/// Quotes one document for a question: how a single product answers.
pub async fn retrieve_doc(
    corpus: &Corpus,
    site: &str,
    doc: &str,
    query: &str,
    k: usize,
) -> Result<Vec<ScoredChunk>, CorpusError> {
    retrieve(corpus, site, query, usize::MAX).await.map(|hits| {
        hits.into_iter()
            .filter(|hit| hit.chunk.doc == doc)
            .take(k)
            .collect()
    })
}

/// Lowercased alphanumeric tokens; short words stay, they ground precisely.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect()
}
