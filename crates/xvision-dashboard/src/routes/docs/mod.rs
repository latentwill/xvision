//! `/api/docs/*` — in-app documentation surface.
//!
//! Implements the `v2a-in-app-docs` contract: surface a curated set of
//! in-repo markdown pages inside the dashboard so first-time operators
//! get Quickstart / Strategies / Scenarios / Eval Runs / CLI Reference
//! without leaving the SPA.
//!
//! Docs are baked into the binary via `include_str!` so the deployed
//! image carries them — no external network fetch at runtime, no
//! `docs/` directory read at startup.
//!
//! Two endpoints:
//!
//! - `GET /api/docs/index` → ordered `[{ slug, title }]` for the
//!   sidebar.
//! - `GET /api/docs/page/:slug` → raw markdown body. `404` for
//!   unknown slugs.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

/// One row in the docs index. `slug` is URL-safe and stable across
/// deploys; `title` is the operator-visible label.
#[derive(Debug, Clone, Serialize)]
pub struct DocPageMeta {
    pub slug: &'static str,
    pub title: &'static str,
}

/// `(slug, title, body)` for every baked page. The order here is the
/// order rendered in the sidebar.
const PAGES: &[(&str, &str, &str)] = &[
    ("quickstart", "Quickstart", include_str!("content/quickstart.md")),
    ("strategies", "Strategies", include_str!("content/strategies.md")),
    ("agents", "Agents", include_str!("content/agents.md")),
    ("scenarios", "Scenarios", include_str!("content/scenarios.md")),
    ("eval-runs", "Eval Runs", include_str!("content/eval-runs.md")),
    ("experiments", "Experiments", include_str!("content/experiments.md")),
    (
        "cli-reference",
        "CLI Reference",
        include_str!("content/cli-reference.md"),
    ),
    (
        "driving-xvn-as-an-agent",
        "Driving xvn as an agent",
        include_str!("content/driving-xvn-as-an-agent.md"),
    ),
];

/// `GET /api/docs/index` — ordered index of all baked pages.
pub async fn index() -> Json<Vec<DocPageMeta>> {
    Json(
        PAGES
            .iter()
            .map(|(slug, title, _)| DocPageMeta { slug, title })
            .collect(),
    )
}

/// `GET /api/docs/page/:slug` — raw markdown body for a single page.
/// Returns `404 Not Found` (with no body) for unknown slugs so the
/// frontend can fall back to the index gracefully.
pub async fn page(Path(slug): Path<String>) -> Result<String, StatusCode> {
    PAGES
        .iter()
        .find(|(s, _, _)| *s == slug)
        .map(|(_, _, body)| (*body).to_string())
        .ok_or(StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Path as AxumPath;

    #[tokio::test]
    async fn index_lists_all_pages_in_order() {
        let Json(rows) = index().await;
        let slugs: Vec<&str> = rows.iter().map(|m| m.slug).collect();
        assert_eq!(
            slugs,
            vec![
                "quickstart",
                "strategies",
                "agents",
                "scenarios",
                "eval-runs",
                "experiments",
                "cli-reference",
                "driving-xvn-as-an-agent",
            ],
            "index must enumerate all baked pages",
        );
        for m in &rows {
            assert!(!m.title.is_empty(), "title must not be empty");
        }
    }

    #[tokio::test]
    async fn page_returns_markdown_for_known_slug() {
        let body = page(AxumPath("quickstart".to_string()))
            .await
            .expect("known slug must resolve");
        assert!(
            body.contains("# Quickstart"),
            "baked content for quickstart should carry its heading",
        );
    }

    #[tokio::test]
    async fn page_returns_404_for_unknown_slug() {
        let err = page(AxumPath("not-a-real-doc".to_string()))
            .await
            .expect_err("unknown slug must 404");
        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn every_baked_page_is_non_empty() {
        for (slug, _, body) in PAGES {
            assert!(
                body.trim().len() > 100,
                "baked content for `{slug}` must be non-trivial",
            );
            assert!(
                body.starts_with("# "),
                "baked content for `{slug}` must start with an h1 heading",
            );
        }
    }
}
