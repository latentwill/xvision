//! Named F+L+T leakage-regression probes for the flywheel plan.
//!
//! This crate owns the structural and temporal substrate:
//! - F: recall reads `Pattern` rows only; `Observation` rows are never
//!   prompt candidates.
//! - T: backtest recall filters Patterns whose `training_window_end`
//!   overlaps the scenario start.
//! - Determinism: identical recall inputs produce identical top-k ids.
//!
//! The L/rhetorical wrapper is in `xvision-engine` because prompt
//! rendering lives in `MemoryRecorder`; `scripts/leakage-regression.sh`
//! runs those engine probes alongside this file.

use chrono::{TimeZone, Utc};
use xvision_memory::store::MemoryStore;
use xvision_memory::types::{MemoryItem, Tier};

fn observation(id: &str, ns: &str, text: &str, embedding: Vec<f32>) -> MemoryItem {
    let source_start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let source_end = Utc.with_ymd_and_hms(2024, 1, 1, 0, 1, 0).unwrap();
    MemoryItem {
        id: id.into(),
        namespace: ns.into(),
        tier: Tier::Observation,
        text: text.into(),
        embedding,
        created_at: source_end,
        run_id: Some("run-leakage".into()),
        scenario_id: Some("scenario-leakage".into()),
        cycle_idx: Some(7),
        source_window_start: Some(source_start),
        source_window_end: Some(source_end),
        training_window_end: None,
        promotion_state: None,
        attestation_id: None,
        forgotten_at: None,
    }
}

fn pattern(id: &str, ns: &str, text: &str, embedding: Vec<f32>) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        namespace: ns.into(),
        tier: Tier::Pattern,
        text: text.into(),
        embedding,
        created_at: Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        run_id: None,
        scenario_id: None,
        cycle_idx: None,
        source_window_start: None,
        source_window_end: None,
        training_window_end: Some(Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap()),
        promotion_state: Some("active".into()),
        attestation_id: None,
        forgotten_at: None,
    }
}

#[tokio::test]
async fn f_structural_recall_never_returns_observations() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .upsert_observation(
            &observation(
                "obs-leak",
                "agent:leakage",
                "OBSERVATION_SHOULD_NOT_RECALL",
                vec![1.0, 0.0],
            ),
            "test",
        )
        .await
        .unwrap();
    store
        .upsert_pattern(
            &pattern(
                "pat-safe",
                "agent:leakage",
                "PATTERN_SHOULD_RECALL",
                vec![1.0, 0.0],
            ),
            "test",
        )
        .await
        .unwrap();

    let scenario_start = Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap();
    let hits = store
        .query("agent:leakage", &[1.0, 0.0], 10, Some(scenario_start))
        .await
        .unwrap();

    assert_eq!(
        hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
        ["pat-safe"]
    );
    assert!(
        hits.iter()
            .all(|h| !h.text.contains("OBSERVATION_SHOULD_NOT_RECALL")),
        "Observation text leaked into recall hits: {hits:?}"
    );
}

#[tokio::test]
async fn t_temporal_filter_blocks_patterns_trained_inside_replay_window() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut inside = pattern(
        "pat-inside-window",
        "agent:leakage",
        "TRAINED_INSIDE_REPLAY_WINDOW",
        vec![1.0, 0.0],
    );
    inside.training_window_end = Some(Utc.with_ymd_and_hms(2024, 9, 1, 0, 0, 0).unwrap());
    let mut before = pattern(
        "pat-before-window",
        "agent:leakage",
        "TRAINED_BEFORE_REPLAY_WINDOW",
        vec![0.9, 0.1],
    );
    before.training_window_end = Some(Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap());
    store.upsert_pattern(&inside, "test").await.unwrap();
    store.upsert_pattern(&before, "test").await.unwrap();

    let scenario_start = Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();
    let hits = store
        .query("agent:leakage", &[1.0, 0.0], 10, Some(scenario_start))
        .await
        .unwrap();

    assert_eq!(
        hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
        ["pat-before-window"]
    );
    assert!(
        hits.iter()
            .all(|h| !h.text.contains("TRAINED_INSIDE_REPLAY_WINDOW")),
        "temporally unsafe Pattern leaked into recall hits: {hits:?}"
    );
}

#[tokio::test]
async fn recall_topk_order_is_deterministic_even_for_equal_scores() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    for id in ["pat-c", "pat-a", "pat-b"] {
        store
            .upsert_pattern(&pattern(id, "agent:determinism", id, vec![1.0, 0.0]), "test")
            .await
            .unwrap();
    }

    let first = store
        .query("agent:determinism", &[1.0, 0.0], 3, None)
        .await
        .unwrap();
    let second = store
        .query("agent:determinism", &[1.0, 0.0], 3, None)
        .await
        .unwrap();
    let first_ids: Vec<_> = first.iter().map(|h| h.id.as_str()).collect();
    let second_ids: Vec<_> = second.iter().map(|h| h.id.as_str()).collect();

    assert_eq!(first_ids, ["pat-a", "pat-b", "pat-c"]);
    assert_eq!(first_ids, second_ids);
}

#[tokio::test]
async fn forgotten_rows_are_hidden_from_recall_but_visible_to_admin_probe() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .upsert_pattern(
            &pattern("pat-forgotten", "agent:leakage", "FORGOTTEN", vec![1.0, 0.0]),
            "test",
        )
        .await
        .unwrap();
    store.demote_pattern("pat-forgotten").await.unwrap();

    let hits = store.query("agent:leakage", &[1.0, 0.0], 10, None).await.unwrap();
    assert!(hits.is_empty(), "forgotten Pattern must not recall: {hits:?}");

    let forgotten = store.count_forgotten("agent:leakage").await.unwrap();
    assert_eq!(forgotten, 1, "admin forgotten probe must see soft-deleted row");
}

#[tokio::test]
async fn observation_write_probe_requires_full_provenance() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let mut missing_run = observation("obs-missing-run", "agent:leakage", "obs", vec![1.0]);
    missing_run.run_id = None;
    assert!(store.upsert_observation(&missing_run, "test").await.is_err());

    let mut missing_window = observation("obs-missing-window", "agent:leakage", "obs", vec![1.0]);
    missing_window.source_window_end = None;
    assert!(store.upsert_observation(&missing_window, "test").await.is_err());

    let valid = observation("obs-valid", "agent:leakage", "obs", vec![1.0]);
    store.upsert_observation(&valid, "test").await.unwrap();
    let row: (Option<String>, Option<String>, Option<i64>) =
        sqlx::query_as("SELECT run_id, scenario_id, cycle_idx FROM memory_items WHERE id = 'obs-valid'")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(row.0.as_deref(), Some("run-leakage"));
    assert_eq!(row.1.as_deref(), Some("scenario-leakage"));
    assert_eq!(row.2, Some(7));
}
