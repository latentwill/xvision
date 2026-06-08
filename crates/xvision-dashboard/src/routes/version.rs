//! `GET /api/version` — build provenance for the running binary.
//!
//! Reports the git SHA + build timestamp baked into the image at
//! `docker buildx build` time (via `--build-arg GIT_SHA/BUILD_TIME` in
//! `scripts/deploy-image.sh`, surfaced as the `XVN_GIT_SHA` /
//! `XVN_BUILT_AT` env vars by `Dockerfile.deploy`), plus the crate
//! version. Lets operators confirm exactly which commit is deployed
//! instead of inferring it from the mutable `:deploy-latest` image tag.
//!
//! Reads the env at request time (not compile time) so the same binary
//! reports correctly regardless of how it was packaged; falls back to
//! `"unknown"` when built outside the image pipeline (e.g. local cargo).

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct VersionInfo {
    /// Full git commit SHA the image was built from, or `"unknown"`.
    pub git_sha: String,
    /// UTC build timestamp (RFC3339), or `"unknown"`.
    pub built_at: String,
    /// `CARGO_PKG_VERSION` of the dashboard crate, baked at compile time.
    pub pkg_version: String,
}

pub async fn version() -> Json<VersionInfo> {
    Json(VersionInfo {
        git_sha: std::env::var("XVN_GIT_SHA").unwrap_or_else(|_| "unknown".into()),
        built_at: std::env::var("XVN_BUILT_AT").unwrap_or_else(|_| "unknown".into()),
        pkg_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
