//! The admitted releases in one SQLite database: bundles, one `current` pointer per
//! site, and the audit log of admissions, publishes and refusals.

use agentrix_site_edge::{Page, Provenance, Release, build_release};
use chrono::{DateTime, Utc};
use thiserror::Error;

/// What the release store can get wrong.
#[derive(Debug, Error)]
pub enum StoreError {
    /// No admitted release has the digest.
    #[error("no admitted release {0}")]
    UnknownRelease(String),
    /// The site has no current release yet.
    #[error("site {0} has no current release")]
    NoCurrent(String),
    /// A bundle did not serialize.
    #[error("release bundle: {0}")]
    Json(#[from] serde_json::Error),
    /// The database failed.
    #[error("release store: {0}")]
    Db(#[from] sqlx::Error),
}

/// One admitted release's metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseMeta {
    /// The release digest.
    pub digest: String,
    /// When it was admitted.
    pub admitted: DateTime<Utc>,
}

/// One audit event: an admission, a publish, or a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEvent {
    /// Sequence within the log.
    pub seq: i64,
    /// When it happened.
    pub at: DateTime<Utc>,
    /// `admitted`, `current`, or `refused`.
    pub kind: String,
    /// The release digest, if any.
    pub digest: Option<String>,
    /// Human detail.
    pub detail: String,
}

/// The admitted releases, the per-site current pointer, and the audit log.
pub struct ReleaseStore {
    pool: sqlx::sqlite::SqlitePool,
}

impl ReleaseStore {
    /// Opens (creating and migrating) the store at `url`.
    pub async fn open(url: &str) -> Result<Self, StoreError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS releases (
                digest TEXT PRIMARY KEY,
                bundle TEXT NOT NULL,
                admitted TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS current (
                site TEXT PRIMARY KEY,
                digest TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS release_events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                at TEXT NOT NULL,
                kind TEXT NOT NULL,
                digest TEXT,
                detail TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    /// Records `release` as admitted. Re-admitting the same digest is a no-op.
    pub async fn admit(&self, release: &Release) -> Result<(), StoreError> {
        let bundle = serde_json::json!({
            "pages": release.pages,
            "provenance": release.provenance,
        });
        sqlx::query("INSERT OR IGNORE INTO releases (digest, bundle, admitted) VALUES (?, ?, ?)")
            .bind(&release.digest)
            .bind(bundle.to_string())
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
        self.log(
            "admitted",
            Some(&release.digest),
            format!("admitted {}", short(&release.digest)),
        )
        .await?;
        Ok(())
    }

    /// The admitted release `digest`, rebuilt with its digest verified.
    pub async fn get(&self, digest: &str) -> Result<Option<Release>, StoreError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT bundle FROM releases WHERE digest = ?")
            .bind(digest)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|(bundle,)| {
            let bundle: serde_json::Value = serde_json::from_str(&bundle)?;
            let pages: Vec<Page> = serde_json::from_value(bundle["pages"].clone())?;
            let provenance: Provenance = serde_json::from_value(bundle["provenance"].clone())?;
            let rebuilt = build_release(pages, &provenance.template, provenance.sources.clone());
            debug_assert_eq!(rebuilt.digest, digest, "stored bundle matches its digest");
            Ok(Release {
                digest: digest.to_string(),
                pages: rebuilt.pages,
                provenance: Provenance {
                    created: provenance.created,
                    ..rebuilt.provenance
                },
            })
        })
        .transpose()
    }

    /// Every admitted release's metadata, oldest first.
    pub async fn list(&self) -> Result<Vec<ReleaseMeta>, StoreError> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT digest, admitted FROM releases ORDER BY rowid")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(|(digest, admitted)| {
                let admitted = admitted.parse::<DateTime<Utc>>().map_err(|e| {
                    StoreError::Db(sqlx::Error::Decode(
                        format!("bad admitted timestamp: {e}").into(),
                    ))
                })?;
                Ok(ReleaseMeta { digest, admitted })
            })
            .collect()
    }

    /// Points `site`'s current at the admitted release `digest`: the one move
    /// publish and rollback share. Refuses unknown digests without moving.
    pub async fn set_current(&self, site: &str, digest: &str) -> Result<Release, StoreError> {
        let Some(release) = self.get(digest).await? else {
            return Err(StoreError::UnknownRelease(digest.to_string()));
        };
        sqlx::query("INSERT INTO current (site, digest) VALUES (?, ?) ON CONFLICT(site) DO UPDATE SET digest = ?")
            .bind(site)
            .bind(digest)
            .bind(digest)
            .execute(&self.pool)
            .await?;
        self.log(
            "current",
            Some(digest),
            format!("{site} now serves {}", short(digest)),
        )
        .await?;
        Ok(release)
    }

    /// The release `site` now serves, or `None` before the first publish.
    pub async fn current(&self, site: &str) -> Result<Option<Release>, StoreError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT digest FROM current WHERE site = ?")
            .bind(site)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some((digest,)) => self.get(&digest).await,
            None => Ok(None),
        }
    }

    /// Records a refusal in the audit log.
    pub async fn log_refusal(
        &self,
        digest: Option<&str>,
        detail: String,
    ) -> Result<(), StoreError> {
        self.log("refused", digest, detail).await
    }

    /// The audit log, oldest first.
    pub async fn events(&self) -> Result<Vec<ReleaseEvent>, StoreError> {
        let rows: Vec<(i64, String, String, Option<String>, String)> =
            sqlx::query_as("SELECT seq, at, kind, digest, detail FROM release_events ORDER BY seq")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(|(seq, at, kind, digest, detail)| {
                let at = at.parse::<DateTime<Utc>>().map_err(|e| {
                    StoreError::Db(sqlx::Error::Decode(
                        format!("bad event timestamp: {e}").into(),
                    ))
                })?;
                Ok(ReleaseEvent {
                    seq,
                    at,
                    kind,
                    digest,
                    detail,
                })
            })
            .collect()
    }

    async fn log(
        &self,
        kind: &str,
        digest: Option<&str>,
        detail: String,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO release_events (at, kind, digest, detail) VALUES (?, ?, ?, ?)")
            .bind(Utc::now().to_rfc3339())
            .bind(kind)
            .bind(digest)
            .bind(detail)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn short(digest: &str) -> &str {
    digest.get(..12).unwrap_or(digest)
}
