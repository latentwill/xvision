//! Provider-side runtime concerns that need network / disk:
//!
//! - `fetcher` — `/v1/models` HTTP clients per provider shape.
//! - `cache`   — on-disk read/write of fetched catalogs.
//! - `service` — `CatalogService`, the process-wide owner of in-memory
//!   + on-disk catalogs and the refresh orchestrator.
//!
//! Pure data types (`Catalog`, `ModelEntry`) live in
//! `xvision_core::providers::catalog` so they're reachable from crates
//! that mustn't take `reqwest` / `tokio::fs` as transitive deps.

pub mod cache;
pub mod fetcher;
pub mod service;

pub use cache::{catalog_cache_dir, is_stale, load as load_cached_catalog, save as save_cached_catalog, DEFAULT_TTL};
pub use fetcher::{build_http_client, fetcher_for, resolve_api_key, CatalogFetcher};
pub use service::CatalogService;
