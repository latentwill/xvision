//! R1 persistence tests for `ReviewAnnotation` + migration 037 schema.
//!
//! Covers:
//!   - `ReviewAnnotation` serde round-trip (Pattern + Risk variants,
//!     with and without `danger`).
//!   - Migration 037 up round-trip: `eval_reviews.annotations` column is
//!     present and defaults to `'[]'` for legacy rows.
//!   - Migration 037 up round-trip: `eval_runs` autofire/review_model/max
//!     columns default correctly for legacy rows.
//!   - `EvalRun` defaults: `auto_fire_review` is `false`, `review_model` is
//!     `None`, after parsing a legacy JSON without those fields.
//!   - `RunStore::create` + `RunStore::get` round-trips the three new fields.
//!   - `RunStore::create_review` + `RunStore::get_review` round-trips the
//!     `annotations` field.

use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use xvision_engine::eval::review::{
    AnnotationAction, AnnotationKind, AnnotationSide, EvalReview, ReviewAnnotation,
    DEFAULT_MAX_ANNOTATIONS_PER_REVIEW,
};
use xvision_engine::eval::{MetricsSummary, ReviewModelRef, Run, RunMode, RunStore};

// ── helpers ─────────────────────────────────────────────────────────────────

/// In-memory pool with the full migration chain that R1 touches. Uses the
/// same pattern as `eval_review.rs` — include_str! applies migrations in
/// order, one query per migration file.
async fn pool_with_037() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/002_eval.sql"),
        include_str!("../migrations/014_eval_agent_id.sql"),
        include_str!("../migrations/015_eval_decisions_reasoning.sql"),
        include_str!("../migrations/016_eval_reviews.sql"),
        include_str!("../migrations/017_eval_findings_review_columns.sql"),
        include_str!("../migrations/022_eval_runs_agents_agent_id.sql"),
        include_str!("../migrations/026_trace_surface_foundation.sql"),
        include_str!("../migrations/027_run_bars_manifest.sql"),
        include_str!("../migrations/031_eval_runs_venue_label.sql"),
        include_str!("../migrations/037_review_annotations_and_eval_autofire.sql"),
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

fn sample_annotation(kind: AnnotationKind, danger: bool) -> ReviewAnnotation {
    ReviewAnnotation {
        idx: 42,
        side: AnnotationSide::Top,
        kind,
        title: "Bull flag breakout".to_string(),
        body: "Strong breakout above consolidation zone with increasing volume.".to_string(),
        conf: 0.82,
        action: AnnotationAction::Long,
        danger,
        ts_sec: 1_700_000_000,
    }
}

// ── serde round-trip tests ───────────────────────────────────────────────────

#[test]
fn review_annotation_pattern_round_trips() {
    let a = sample_annotation(AnnotationKind::Pattern, false);
    let json = serde_json::to_string(&a).unwrap();
    let back: ReviewAnnotation = serde_json::from_str(&json).unwrap();
    assert_eq!(back, a);
}

#[test]
fn review_annotation_risk_with_danger_round_trips() {
    let a = sample_annotation(AnnotationKind::Risk, true);
    let json = serde_json::to_string(&a).unwrap();
    let back: ReviewAnnotation = serde_json::from_str(&json).unwrap();
    assert_eq!(back, a);
    assert!(back.danger);
}

#[test]
fn review_annotation_danger_defaults_to_false_when_absent() {
    // Simulate a JSON payload that does NOT include "danger" (e.g., from an
    // older version of the producer or a hand-crafted test fixture).
    let json = r#"{
        "idx": 7,
        "side": "bottom",
        "type": "FLOW",
        "title": "Momentum shift",
        "body": "Price action rolling over at resistance.",
        "conf": 0.71,
        "action": "CAUTION",
        "ts_sec": 1700001234
    }"#;
    let a: ReviewAnnotation = serde_json::from_str(json).unwrap();
    assert!(!a.danger, "danger should default to false when absent in JSON");
    assert_eq!(a.kind, AnnotationKind::Flow);
    assert_eq!(a.action, AnnotationAction::Caution);
    assert_eq!(a.side, AnnotationSide::Bottom);
}

#[test]
fn annotation_kind_serialises_uppercase() {
    assert_eq!(
        serde_json::to_string(&AnnotationKind::Pattern).unwrap(),
        "\"PATTERN\""
    );
    assert_eq!(serde_json::to_string(&AnnotationKind::Risk).unwrap(), "\"RISK\"");
    assert_eq!(
        serde_json::to_string(&AnnotationKind::Reversion).unwrap(),
        "\"REVERSION\""
    );
    assert_eq!(
        serde_json::to_string(&AnnotationKind::Structure).unwrap(),
        "\"STRUCTURE\""
    );
    assert_eq!(serde_json::to_string(&AnnotationKind::Flow).unwrap(), "\"FLOW\"");
}

#[test]
fn annotation_action_serialises_uppercase() {
    assert_eq!(
        serde_json::to_string(&AnnotationAction::Watch).unwrap(),
        "\"WATCH\""
    );
    assert_eq!(
        serde_json::to_string(&AnnotationAction::Long).unwrap(),
        "\"LONG\""
    );
    assert_eq!(
        serde_json::to_string(&AnnotationAction::Short).unwrap(),
        "\"SHORT\""
    );
    assert_eq!(
        serde_json::to_string(&AnnotationAction::Caution).unwrap(),
        "\"CAUTION\""
    );
}

#[test]
fn annotation_side_serialises_lowercase() {
    assert_eq!(serde_json::to_string(&AnnotationSide::Top).unwrap(), "\"top\"");
    assert_eq!(
        serde_json::to_string(&AnnotationSide::Bottom).unwrap(),
        "\"bottom\""
    );
}

#[test]
fn default_max_annotations_is_eight() {
    assert_eq!(DEFAULT_MAX_ANNOTATIONS_PER_REVIEW, 8);
}

// ── EvalRun defaults ─────────────────────────────────────────────────────────

#[test]
fn eval_run_auto_fire_review_defaults_false_in_legacy_json() {
    // JSON without the new R1 fields — simulates a row persisted before 037.
    let legacy = r#"{
        "id": "01HWZZ0000000000000000001",
        "agent_id": "bundle-hash-abc",
        "scenario_id": "crypto-bull-q1-2025",
        "mode": "backtest",
        "status": "completed",
        "started_at": "2026-05-01T00:00:00Z"
    }"#;
    let run: Run = serde_json::from_str(legacy).unwrap();
    assert!(
        !run.auto_fire_review,
        "auto_fire_review must default to false for legacy JSON"
    );
    assert!(
        run.review_model.is_none(),
        "review_model must default to None for legacy JSON"
    );
    assert!(
        run.max_annotations_per_review.is_none(),
        "max_annotations_per_review must default to None for legacy JSON"
    );
}

#[test]
fn eval_run_new_queued_has_correct_defaults() {
    let r = Run::new_queued("hash".into(), "scenario".into(), RunMode::Backtest);
    assert!(!r.auto_fire_review);
    assert!(r.review_model.is_none());
    assert!(r.max_annotations_per_review.is_none());
}

// ── migration 037 integration tests ─────────────────────────────────────────

#[tokio::test]
async fn migration_037_adds_annotations_column_with_empty_default() {
    let pool = pool_with_037().await;

    // Disable FK enforcement for this raw-insert test — the column default
    // check does not require a real parent eval_run row.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();

    // Insert a row directly without specifying `annotations` — the column
    // default ('[]') must apply.
    sqlx::query(
        "INSERT INTO eval_reviews \
         (id, eval_run_id, agent_profile_id, status, created_at, updated_at) \
         VALUES ('rev-1', 'run-1', 'fast-trader-agent', 'queued', \
                 '2026-05-23T00:00:00Z', '2026-05-23T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let annotations_raw: String =
        sqlx::query_scalar("SELECT annotations FROM eval_reviews WHERE id = 'rev-1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(annotations_raw, "[]", "annotations column must default to '[]'");
}

#[tokio::test]
async fn migration_037_adds_autofire_column_with_zero_default() {
    let pool = pool_with_037().await;

    // Insert a run without specifying auto_fire_review — must default to 0.
    sqlx::query(
        "INSERT INTO eval_runs \
         (id, agent_id, scenario_id, mode, status, started_at) \
         VALUES ('run-2', 'h', 'scenario', 'backtest', 'queued', '2026-05-23T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let auto_fire: i64 = sqlx::query_scalar("SELECT auto_fire_review FROM eval_runs WHERE id = 'run-2'")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(auto_fire, 0, "auto_fire_review must default to 0");
}

// ── RunStore round-trip ──────────────────────────────────────────────────────

#[tokio::test]
async fn run_store_round_trips_autofire_and_review_model() {
    let pool = pool_with_037().await;
    let store = RunStore::new(pool);

    let mut run = Run::new_queued(
        "agent-abc".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    run.auto_fire_review = true;
    run.review_model = Some(ReviewModelRef {
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
    });
    run.max_annotations_per_review = Some(5);

    store.create(&run).await.unwrap();
    let loaded = store.get(&run.id).await.unwrap();

    assert!(loaded.auto_fire_review);
    let model = loaded.review_model.unwrap();
    assert_eq!(model.provider, "anthropic");
    assert_eq!(model.model, "claude-sonnet-4-6");
    assert_eq!(loaded.max_annotations_per_review, Some(5));
}

#[tokio::test]
async fn run_store_round_trips_null_review_model() {
    let pool = pool_with_037().await;
    let store = RunStore::new(pool);

    let run = Run::new_queued("agent-xyz".into(), "crypto-bull-q1-2025".into(), RunMode::Paper);
    // auto_fire_review = false (default), review_model = None (default)
    store.create(&run).await.unwrap();
    let loaded = store.get(&run.id).await.unwrap();

    assert!(!loaded.auto_fire_review);
    assert!(loaded.review_model.is_none());
    assert!(loaded.max_annotations_per_review.is_none());
}

#[tokio::test]
async fn run_store_round_trips_review_annotations() {
    let pool = pool_with_037().await;
    let store = RunStore::new(pool);

    // Create a parent run first (FK).
    let run = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    store.create(&run).await.unwrap();

    // Simulate a finalized review with two annotations.
    let mut review = EvalReview::new_queued(run.id.clone(), "fast-trader-agent".into());
    review.annotations = vec![
        sample_annotation(AnnotationKind::Pattern, false),
        sample_annotation(AnnotationKind::Risk, true),
    ];

    store.create_review(&review).await.unwrap();
    let loaded = store.get_review(&review.id).await.unwrap().unwrap();

    assert_eq!(loaded.annotations.len(), 2);
    assert_eq!(loaded.annotations[0].kind, AnnotationKind::Pattern);
    assert!(!loaded.annotations[0].danger);
    assert_eq!(loaded.annotations[1].kind, AnnotationKind::Risk);
    assert!(loaded.annotations[1].danger);
}

#[tokio::test]
async fn run_store_set_review_annotations_replaces_existing() {
    let pool = pool_with_037().await;
    let store = RunStore::new(pool);

    let run = Run::new_queued("agent-y".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    store.create(&run).await.unwrap();

    let review = EvalReview::new_queued(run.id.clone(), "reasoning-agent".into());
    store.create_review(&review).await.unwrap();

    // Initially empty.
    let loaded = store.get_review(&review.id).await.unwrap().unwrap();
    assert!(loaded.annotations.is_empty());

    // Set annotations via the helper.
    let new_annotations = vec![sample_annotation(AnnotationKind::Flow, false)];
    store
        .set_review_annotations(&review.id, &new_annotations)
        .await
        .unwrap();

    let reloaded = store.get_review(&review.id).await.unwrap().unwrap();
    assert_eq!(reloaded.annotations.len(), 1);
    assert_eq!(reloaded.annotations[0].kind, AnnotationKind::Flow);
}
