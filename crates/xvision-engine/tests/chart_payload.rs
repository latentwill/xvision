//! Integration tests for `api::chart::build_run_payload`.
//!
//! Full end-to-end testing (bars + decisions + equity fully seeded) requires
//! a live Alpaca connection and is covered by the Task 15 smoke test. The
//! tests here focus on the boundary conditions that don't need network access.

use tempfile::tempdir;
use xvision_engine::api::{Actor, ApiContext, ApiError};

/// Build a fresh `ApiContext` backed by an on-disk SQLite DB under a tmpdir.
/// Uses `ApiContext::open` so all migrations are applied and the canonical
/// scenario seed runs — identical to the pattern in `scenario_api.rs` and
/// `api_context.rs`.
async fn test_ctx() -> ApiContext {
    let dir = Box::leak(Box::new(tempdir().unwrap()));
    ApiContext::open(
        dir.path(),
        Actor::Cli {
            user: "chart-test".into(),
        },
    )
    .await
    .unwrap()
}

/// `build_run_payload` must return `ApiError::NotFound` for a run id that
/// doesn't exist in the store.
#[tokio::test]
async fn build_run_payload_unknown_run_returns_not_found() {
    let ctx = test_ctx().await;
    let err = xvision_engine::api::chart::build_run_payload(&ctx, "r_does_not_exist")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected NotFound, got: {err:?}"
    );
    // The error message should name the offending run id.
    let msg = err.to_string();
    assert!(
        msg.contains("r_does_not_exist"),
        "NotFound message should include the run id, got: {msg}"
    );
}

// ── Task 4 — build_compare_payload tests ────────────────────────────────────

/// Requesting more than 10 runs must return `ApiError::Validation` whose
/// message contains "narrow your filter".
#[tokio::test]
async fn build_compare_payload_caps_at_10_runs() {
    let ctx = test_ctx().await;
    let ids: Vec<String> = (0..11).map(|i| format!("r_{i}")).collect();
    let err = xvision_engine::api::chart::build_compare_payload(&ctx, &ids)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ApiError::Validation(_)),
        "expected Validation error for >10 runs, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("narrow your filter"),
        "error message must contain 'narrow your filter', got: {msg}"
    );
}

// ── Task 1 — build_scenario_payload tests ───────────────────────────────────

/// Canonical scenarios have no cached bars in a fresh xvn_home — the cache
/// is only populated when `xvn bars fetch` runs. So `build_scenario_payload`
/// must return `CacheStatus::NotCached`.
#[tokio::test]
async fn build_scenario_payload_returns_not_cached_for_seeded_scenario() {
    use xvision_engine::api::chart::{build_scenario_payload, CacheStatus};
    let ctx = test_ctx().await;
    let payload = build_scenario_payload(&ctx, "crypto-bull-q1-2025")
        .await
        .unwrap();
    assert_eq!(payload.scenario.id, "crypto-bull-q1-2025");
    assert!(
        matches!(payload.cache_status, CacheStatus::NotCached { .. }),
        "expected NotCached on fresh DB, got: {:?}",
        payload.cache_status
    );
    assert!(payload.bars.is_empty(), "bars should be empty for NotCached");
}

/// `build_scenario_payload` must return `ApiError::NotFound` for an id that
/// does not exist in the scenarios table.
#[tokio::test]
async fn build_scenario_payload_returns_not_found_for_unknown() {
    use xvision_engine::api::chart::build_scenario_payload;
    let ctx = test_ctx().await;
    let err = build_scenario_payload(&ctx, "no-such-scenario")
        .await
        .unwrap_err();
    assert!(
        matches!(err, xvision_engine::api::ApiError::NotFound(_)),
        "expected NotFound, got: {err:?}"
    );
}

// ── Task 2 — build_strategy_payload tests ───────────────────────────────────

/// A strategy id with no runs must return an empty `run_series`.
#[tokio::test]
async fn build_strategy_payload_empty_for_unused_strategy() {
    use xvision_engine::api::chart::build_strategy_payload;
    let ctx = test_ctx().await;
    let payload = build_strategy_payload(&ctx, "unused-strategy")
        .await
        .unwrap();
    assert!(
        payload.run_series.is_empty(),
        "expected no runs for unused strategy"
    );
}

/// With 0 runs in the DB, the first missing id should return NotFound.
#[tokio::test]
async fn build_compare_payload_returns_not_found_for_missing_run() {
    let ctx = test_ctx().await;
    // Single missing id — should get NotFound (not panic or internal error).
    let ids = vec!["r_missing_1".to_string()];
    let err = xvision_engine::api::chart::build_compare_payload(&ctx, &ids)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected NotFound for missing run id, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("r_missing_1"),
        "error message should name the missing run id, got: {msg}"
    );
}
