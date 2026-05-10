//! Command-palette (⌘K) full-text search backbone — Plan #12.
//!
//! Owns migration `004_search_index.sql` per the v1 migration registry. This
//! module exposes the `SearchIndex` CRUD over the FTS5 virtual table; the
//! per-artifact indexer hooks (bundle::save, eval::store::finalize,
//! findings::record, etc.) and the `engine::api::search::*` surface that
//! wraps them are deferred to follow-up PRs.
//!
//! Why this lands now: shipping the migration + the index API early means
//! every future writer can become ⌘K-searchable with a single
//! `SearchIndex::upsert(pool, &entry).await?` call in its success path. No
//! retroactive migration coordination required.

pub mod index;

pub use index::{IndexEntry, SearchHit, SearchIndex, SearchKind, SearchQuery};
