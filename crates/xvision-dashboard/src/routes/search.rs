//! `GET /api/search` — command-palette (⌘K) full-text search.
//!
//! Thin wrapper over `engine::api::search::search`. Empty `q` returns the
//! most-recently-touched artifacts so the palette can render a useful
//! "just-opened" state. `kind=` filters to a single artifact kind;
//! anything else surfaces as `400 validation`. `limit` defaults to 50 and
//! is hard-capped at 200 server-side so a malformed client can't drag
//! down the engine pool.

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::search as api_search;
use xvision_engine::search::{SearchHit, SearchKind, SearchQuery};

use crate::error::DashboardError;
use crate::state::AppState;

const MAX_LIMIT: u32 = 200;

#[derive(Debug, Default, Deserialize)]
pub struct SearchParams {
    /// Free-form query string. Empty string → recency listing.
    pub q: Option<String>,
    /// Optional single-kind filter. Multi-kind filtering would need a
    /// list-typed query param; v1 keeps this scalar to match the way the
    /// modal renders one group per kind regardless.
    pub kind: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
}

pub async fn handler(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, DashboardError> {
    let kind = params
        .kind
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| {
            SearchKind::parse(s).ok_or_else(|| DashboardError::Validation {
                field: "kind".into(),
                msg: format!("unknown search kind '{s}'"),
            })
        })
        .transpose()?;
    let limit = params.limit.map(|n| n.min(MAX_LIMIT));
    let opts = SearchQuery { kind, limit };
    let q = params.q.unwrap_or_default();
    let hits = api_search::search(&state.api_context(), &q, &opts).await?;
    Ok(Json(SearchResponse { hits }))
}
