//! `SearchIndex` — async sqlx wrapper around the `search_index` FTS5 table.
//! Indexer hooks (one per artifact kind) call `upsert` in their success path
//! and `delete` when an artifact is removed. The query side is `search`,
//! which resolves to a single `SELECT … MATCH ?` against the FTS5 virtual
//! table.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Row tuple returned by `search_index` FTS5 SELECT queries.
type SearchRow = (String, String, String, String, String, String, String, f64);

/// Kinds of artifacts indexed by ⌘K. New variants land alongside their
/// indexer hooks. Always serializes to a stable lowercase string —
/// matches FTS5 storage and downstream UI grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Strategy,
    Run,
    Finding,
    Scenario,
    Deployment,
    JournalEntry,
    /// Static "named action" rows seeded once at dashboard startup
    /// (e.g. "New strategy from template…"). Behaves identically to other
    /// kinds for the index/search surface; the UI surfaces them in a
    /// dedicated "Actions" group at the top of the result list.
    Action,
}

impl SearchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SearchKind::Strategy => "strategy",
            SearchKind::Run => "run",
            SearchKind::Finding => "finding",
            SearchKind::Scenario => "scenario",
            SearchKind::Deployment => "deployment",
            SearchKind::JournalEntry => "journal_entry",
            SearchKind::Action => "action",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "strategy" => SearchKind::Strategy,
            "run" => SearchKind::Run,
            "finding" => SearchKind::Finding,
            "scenario" => SearchKind::Scenario,
            "deployment" => SearchKind::Deployment,
            "journal_entry" => SearchKind::JournalEntry,
            "action" => SearchKind::Action,
            _ => return None,
        })
    }
}

/// One indexable row. Indexers build this from the artifact's authoritative
/// state and pass it to `upsert`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub artifact_id: String,
    pub kind: SearchKind,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub updated_at: DateTime<Utc>,
    /// In-app URL the result row links to (e.g. `/eval/runs/<id>`).
    pub href: String,
}

/// One result row returned from `search`. `bm25_score` is the raw FTS5 BM25
/// rank — lower is better; UI sort can simply order ascending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub artifact_id: String,
    pub kind: SearchKind,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub updated_at: DateTime<Utc>,
    pub href: String,
    pub bm25_score: f64,
}

/// Optional knobs for `search`. `kind` filters to a single artifact kind;
/// `limit` caps the result count.
#[derive(Debug, Default, Clone)]
pub struct SearchQuery {
    pub kind: Option<SearchKind>,
    pub limit: Option<u32>,
}

/// Stateless CRUD over the `search_index` FTS5 virtual table.
///
/// FTS5 is a virtual table and does NOT support `INSERT … ON CONFLICT`
/// or `UNIQUE` constraints, so a literal single-statement upsert isn't
/// possible at the SQL layer. The previous implementation did a
/// deferred-transaction delete-then-insert which raced with eval
/// finalize under concurrent writers (audit log:
/// `xvision_engine::api::search: search index upsert (run) failed
/// error=delete prior row run_id=01KS09WVDZH1F01TW8527RXYED`,
/// intake #344). The fix here is two-fold:
///
///   1. Open the transaction with `BEGIN IMMEDIATE` so the write lock
///      is acquired up-front; this eliminates the deferred→immediate
///      lock upgrade that was failing under concurrent writers.
///   2. Issue both statements in the same transaction so a concurrent
///      reader can never observe the "deleted but not yet
///      re-inserted" intermediate state.
///
/// The result is an idempotent, race-free upsert without the new
/// migration that an `ON CONFLICT`-capable rowid table would require.
pub struct SearchIndex;

const DEFAULT_LIMIT: u32 = 50;
/// How many times to retry `upsert` on transient `SQLITE_BUSY` /
/// `SQLITE_LOCKED` errors. The eval finalize path is the only
/// remaining contender for the write lock; a couple of retries
/// at a short backoff is enough in practice.
const UPSERT_BUSY_RETRIES: u32 = 4;

impl SearchIndex {
    /// Insert or replace a row keyed by `(artifact_id, kind)`. The two
    /// SQL statements share a `BEGIN IMMEDIATE` transaction so the
    /// write lock is held end-to-end and concurrent writers never
    /// observe a partial state.
    pub async fn upsert(pool: &SqlitePool, entry: &IndexEntry) -> Result<()> {
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..=UPSERT_BUSY_RETRIES {
            match Self::upsert_once(pool, entry).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // Only retry on SQLITE_BUSY / SQLITE_LOCKED. Any
                    // other error (schema mismatch, bad data, ...) is
                    // a real bug and should surface immediately.
                    if !is_busy_error(&e) || attempt == UPSERT_BUSY_RETRIES {
                        return Err(e);
                    }
                    tracing::warn!(
                        target: "xvision::search",
                        attempt = attempt + 1,
                        error = %e,
                        artifact_id = %entry.artifact_id,
                        kind = entry.kind.as_str(),
                        "search index upsert busy; retrying"
                    );
                    // Cheap exponential-ish backoff (10ms, 20ms, 40ms,
                    // 80ms). No `rand` dep — the contention pattern is
                    // not pathological enough to need jitter.
                    let delay = std::time::Duration::from_millis(10u64 << attempt);
                    tokio::time::sleep(delay).await;
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("search upsert exhausted retries")))
    }

    async fn upsert_once(pool: &SqlitePool, entry: &IndexEntry) -> Result<()> {
        // Acquire a dedicated connection so we can issue `BEGIN
        // IMMEDIATE` (sqlx's `pool.begin()` issues `BEGIN` which is
        // `DEFERRED` by default — that's the upgrade-deadlock that
        // produced the audit-log race).
        let mut conn = pool.acquire().await.context("acquire connection for upsert")?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .context("begin immediate tx for upsert")?;
        let delete_res = sqlx::query("DELETE FROM search_index WHERE artifact_id = ?1 AND kind = ?2")
            .bind(&entry.artifact_id)
            .bind(entry.kind.as_str())
            .execute(&mut *conn)
            .await;
        if let Err(e) = delete_res {
            // Best-effort rollback; surface the original error either way.
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(anyhow::Error::from(e).context("delete prior row"));
        }
        let insert_res = sqlx::query(
            "INSERT INTO search_index (artifact_id, kind, title, summary, tags, updated_at, href) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&entry.artifact_id)
        .bind(entry.kind.as_str())
        .bind(&entry.title)
        .bind(&entry.summary)
        .bind(entry.tags.join(" "))
        .bind(entry.updated_at.to_rfc3339())
        .bind(&entry.href)
        .execute(&mut *conn)
        .await;
        if let Err(e) = insert_res {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(anyhow::Error::from(e).context("insert search_index row"));
        }
        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .context("commit upsert tx")?;
        Ok(())
    }

    /// Return all artifact IDs of the given kind, newest-first.
    pub async fn list_ids(pool: &SqlitePool, kind: SearchKind) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT artifact_id FROM search_index WHERE kind = ?1 ORDER BY updated_at DESC",
        )
        .bind(kind.as_str())
        .fetch_all(pool)
        .await
        .context("list search_index ids")?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Remove a row by `(artifact_id, kind)`. No-op if missing.
    pub async fn delete(pool: &SqlitePool, kind: SearchKind, artifact_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM search_index WHERE artifact_id = ?1 AND kind = ?2")
            .bind(artifact_id)
            .bind(kind.as_str())
            .execute(pool)
            .await
            .context("delete search_index row")?;
        Ok(())
    }

    /// Run an FTS5 MATCH query. Empty queries fall back to a recency
    /// listing so the UI's "just opened, no input yet" state can show the
    /// most-recently-touched artifacts.
    pub async fn search(pool: &SqlitePool, q: &str, opts: &SearchQuery) -> Result<Vec<SearchHit>> {
        let limit = opts.limit.unwrap_or(DEFAULT_LIMIT) as i64;
        let trimmed = q.trim();
        if trimmed.is_empty() {
            return Self::recent(pool, opts, limit).await;
        }

        // FTS5 expects column-prefixed terms (or none); the porter tokenizer
        // matches stems. We pass the raw query as MATCH input but escape
        // double quotes to keep injection-style payloads from breaking the
        // FTS5 grammar. (Not a security concern — search results are
        // read-only — but it surfaces clearer "no hits" when users paste
        // text that happens to contain quotes.)
        let escaped = trimmed.replace('"', "\"\"");
        let match_arg = format!("\"{escaped}\"");

        let rows: Vec<SearchRow> = match opts.kind {
            Some(kind) => sqlx::query_as(
                "SELECT artifact_id, kind, title, summary, tags, updated_at, href, bm25(search_index) \
                 FROM search_index \
                 WHERE search_index MATCH ?1 AND kind = ?2 \
                 ORDER BY bm25(search_index), updated_at DESC \
                 LIMIT ?3",
            )
            .bind(&match_arg)
            .bind(kind.as_str())
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("FTS5 MATCH (kind-filtered)")?,
            None => sqlx::query_as(
                "SELECT artifact_id, kind, title, summary, tags, updated_at, href, bm25(search_index) \
                 FROM search_index \
                 WHERE search_index MATCH ?1 \
                 ORDER BY bm25(search_index), updated_at DESC \
                 LIMIT ?2",
            )
            .bind(&match_arg)
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("FTS5 MATCH (all kinds)")?,
        };

        rows.into_iter().map(parse_row).collect()
    }

    async fn recent(pool: &SqlitePool, opts: &SearchQuery, limit: i64) -> Result<Vec<SearchHit>> {
        let rows: Vec<(String, String, String, String, String, String, String)> = match opts.kind {
            Some(kind) => sqlx::query_as(
                "SELECT artifact_id, kind, title, summary, tags, updated_at, href \
                 FROM search_index \
                 WHERE kind = ?1 \
                 ORDER BY updated_at DESC \
                 LIMIT ?2",
            )
            .bind(kind.as_str())
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("recent rows (kind-filtered)")?,
            None => sqlx::query_as(
                "SELECT artifact_id, kind, title, summary, tags, updated_at, href \
                 FROM search_index \
                 ORDER BY updated_at DESC \
                 LIMIT ?1",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("recent rows (all kinds)")?,
        };
        rows.into_iter()
            .map(|(artifact_id, kind, title, summary, tags, updated_at, href)| {
                Ok(SearchHit {
                    artifact_id,
                    kind: SearchKind::parse(&kind).context("unknown kind in row")?,
                    title,
                    summary,
                    tags: split_tags(&tags),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at)
                        .context("parse updated_at")?
                        .with_timezone(&Utc),
                    href,
                    bm25_score: 0.0,
                })
            })
            .collect()
    }
}

fn parse_row(
    (artifact_id, kind, title, summary, tags, updated_at, href, bm25_score): (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        f64,
    ),
) -> Result<SearchHit> {
    Ok(SearchHit {
        artifact_id,
        kind: SearchKind::parse(&kind).context("unknown kind in row")?,
        title,
        summary,
        tags: split_tags(&tags),
        updated_at: DateTime::parse_from_rfc3339(&updated_at)
            .context("parse updated_at")?
            .with_timezone(&Utc),
        href,
        bm25_score,
    })
}

fn split_tags(joined: &str) -> Vec<String> {
    joined.split_whitespace().map(str::to_string).collect()
}

/// Detect `SQLITE_BUSY` / `SQLITE_LOCKED` so `upsert` can retry transient
/// contention without papering over real errors. sqlx surfaces these as
/// `sqlx::Error::Database` with extended code `5` / `6` (SQLite-native).
fn is_busy_error(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(sqlx_err) = cause.downcast_ref::<sqlx::Error>() {
            if let Some(db_err) = sqlx_err.as_database_error() {
                if let Some(code) = db_err.code() {
                    // SQLite primary codes: 5 = SQLITE_BUSY, 6 = SQLITE_LOCKED.
                    if code == "5" || code == "6" {
                        return true;
                    }
                }
                // Fallback: the message is a stable enough surface for
                // mocked / older sqlx versions.
                let msg = db_err.message().to_lowercase();
                if msg.contains("database is locked") || msg.contains("database table is locked") {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(include_str!("../../migrations/004_search_index.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn entry(id: &str, kind: SearchKind, title: &str, summary: &str, tags: &[&str]) -> IndexEntry {
        IndexEntry {
            artifact_id: id.into(),
            kind,
            title: title.into(),
            summary: summary.into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            updated_at: Utc::now(),
            href: format!("/{}/{id}", kind.as_str()),
        }
    }

    #[tokio::test]
    async fn upsert_and_search_by_title() {
        let pool = fresh_pool().await;
        SearchIndex::upsert(
            &pool,
            &entry(
                "btc-momentum",
                SearchKind::Strategy,
                "btc-momentum",
                "Trend follower on BTC perp",
                &["trend"],
            ),
        )
        .await
        .unwrap();
        let hits = SearchIndex::search(&pool, "btc", &SearchQuery::default())
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].artifact_id, "btc-momentum");
        assert_eq!(hits[0].kind, SearchKind::Strategy);
    }

    #[tokio::test]
    async fn upsert_dedupes() {
        let pool = fresh_pool().await;
        let mut e = entry("s1", SearchKind::Strategy, "first title", "first", &[]);
        SearchIndex::upsert(&pool, &e).await.unwrap();
        e.title = "second title".into();
        e.summary = "second".into();
        SearchIndex::upsert(&pool, &e).await.unwrap();

        let hits = SearchIndex::search(&pool, "title", &SearchQuery::default())
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "second title");
    }

    #[tokio::test]
    async fn search_by_tag() {
        let pool = fresh_pool().await;
        SearchIndex::upsert(
            &pool,
            &entry(
                "s1",
                SearchKind::Strategy,
                "alpha",
                "irrelevant",
                &["mean-reversion", "btc"],
            ),
        )
        .await
        .unwrap();
        let hits = SearchIndex::search(&pool, "mean-reversion", &SearchQuery::default())
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].tags.contains(&"mean-reversion".to_string()));
    }

    #[tokio::test]
    async fn kind_filter_excludes_other_kinds() {
        let pool = fresh_pool().await;
        SearchIndex::upsert(&pool, &entry("s1", SearchKind::Strategy, "btc thing", "x", &[]))
            .await
            .unwrap();
        SearchIndex::upsert(&pool, &entry("r1", SearchKind::Run, "btc thing", "x", &[]))
            .await
            .unwrap();

        let only_runs = SearchIndex::search(
            &pool,
            "btc",
            &SearchQuery {
                kind: Some(SearchKind::Run),
                limit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(only_runs.len(), 1);
        assert_eq!(only_runs[0].kind, SearchKind::Run);
    }

    #[tokio::test]
    async fn empty_query_returns_recent_rows() {
        let pool = fresh_pool().await;
        let mut e1 = entry("s1", SearchKind::Strategy, "old", "x", &[]);
        e1.updated_at = Utc::now() - chrono::Duration::hours(1);
        let mut e2 = entry("s2", SearchKind::Strategy, "new", "x", &[]);
        e2.updated_at = Utc::now();
        SearchIndex::upsert(&pool, &e1).await.unwrap();
        SearchIndex::upsert(&pool, &e2).await.unwrap();

        let hits = SearchIndex::search(&pool, "", &SearchQuery::default())
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].artifact_id, "s2"); // newest first
    }

    #[tokio::test]
    async fn delete_removes_row() {
        let pool = fresh_pool().await;
        SearchIndex::upsert(&pool, &entry("s1", SearchKind::Strategy, "doomed", "x", &[]))
            .await
            .unwrap();
        SearchIndex::delete(&pool, SearchKind::Strategy, "s1")
            .await
            .unwrap();
        let hits = SearchIndex::search(&pool, "doomed", &SearchQuery::default())
            .await
            .unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn list_ids_returns_indexed_strategies_newest_first() {
        let pool = fresh_pool().await;

        let mut e1 = entry("strat-a", SearchKind::Strategy, "alpha", "x", &[]);
        e1.updated_at = Utc::now() - chrono::Duration::hours(1);
        let mut e2 = entry("strat-b", SearchKind::Strategy, "bravo", "x", &[]);
        e2.updated_at = Utc::now();
        SearchIndex::upsert(&pool, &e1).await.unwrap();
        SearchIndex::upsert(&pool, &e2).await.unwrap();

        // Also insert a run row — must not appear in strategy list.
        let e_run = entry("run-x", SearchKind::Run, "some run", "x", &[]);
        SearchIndex::upsert(&pool, &e_run).await.unwrap();

        let ids = SearchIndex::list_ids(&pool, SearchKind::Strategy).await.unwrap();
        assert_eq!(ids, vec!["strat-b", "strat-a"], "newest first, runs excluded");
    }

    #[tokio::test]
    async fn list_ids_empty_when_none_indexed() {
        let pool = fresh_pool().await;
        let ids = SearchIndex::list_ids(&pool, SearchKind::Strategy).await.unwrap();
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn list_ids_excludes_deleted_entry() {
        let pool = fresh_pool().await;
        SearchIndex::upsert(&pool, &entry("s1", SearchKind::Strategy, "keep", "x", &[]))
            .await
            .unwrap();
        SearchIndex::upsert(&pool, &entry("s2", SearchKind::Strategy, "gone", "x", &[]))
            .await
            .unwrap();
        SearchIndex::delete(&pool, SearchKind::Strategy, "s2").await.unwrap();

        let ids = SearchIndex::list_ids(&pool, SearchKind::Strategy).await.unwrap();
        assert_eq!(ids, vec!["s1"]);
    }
}
