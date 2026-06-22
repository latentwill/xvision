//! Integration tests for strategy-id path safety (QA finding P3-strategy-id).
//!
//! The `FilesystemStore` validates every caller-supplied id through
//! `strategies::id::validate_strategy_id_for_path` before joining a path.
//! These tests confirm the invariant holds at the store boundary AND
//! through the public `xvision_engine::strategies::store` API.

use tempfile::tempdir;
use xvision_engine::strategies::id::{validate_strategy_id_for_path, StrategyIdError};
use xvision_engine::strategies::manifest::PublicManifest;
use xvision_engine::strategies::risk::RiskPreset;
use xvision_engine::strategies::store::{FilesystemStore, StrategyStore};
use xvision_engine::strategies::Strategy;

fn strategy_with_id(id: &str) -> Strategy {
    Strategy {
        manifest: PublicManifest {
            id: id.to_string(),
            display_name: "t".into(),
            plain_summary: "t".into(),
            creator: "@t".into(),
            template: "trend_follower".into(),
            regime_fit: vec![],
            asset_universe: vec![],
            decision_cadence_minutes: 60,
            attested_with: vec![],
            required_tools: vec![],
            risk_preset_or_config: "balanced".into(),
            published_at: None,
            min_warmup_bars: None,
            color: None,
            execution_mode: Default::default(),
            capital_mode: Default::default(),
            timeframe_requirements: Default::default(),
        },
        hypothesis: None,
        agents: vec![],
        pipeline: Default::default(),
        regime_slot: None,
        trader_slot: None,
        risk: RiskPreset::Balanced.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    }
}

fn assert_store_root_unchanged(root: &std::path::Path, expected_entries: &[&str]) {
    let mut found: Vec<String> = std::fs::read_dir(root)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    found.sort();
    let mut expected: Vec<String> = expected_entries.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(found, expected, "store root contents changed unexpectedly");
}

// ── Pure validator coverage ───────────────────────────────────────────

#[test]
fn validator_accepts_ulid() {
    let id = "01HZSTRATEGY00000000000000";
    assert_eq!(validate_strategy_id_for_path(id), Ok(id));
}

#[test]
fn validator_accepts_alphanumeric_with_separators() {
    let id = "btc-momentum_v2";
    assert_eq!(validate_strategy_id_for_path(id), Ok(id));
}

#[test]
fn validator_rejects_empty() {
    assert_eq!(validate_strategy_id_for_path(""), Err(StrategyIdError::Empty));
}

#[test]
fn validator_rejects_double_dot() {
    assert_eq!(
        validate_strategy_id_for_path(".."),
        Err(StrategyIdError::ReservedSegment),
    );
}

#[test]
fn validator_rejects_single_dot() {
    assert_eq!(
        validate_strategy_id_for_path("."),
        Err(StrategyIdError::ReservedSegment),
    );
}

#[test]
fn validator_rejects_forward_slash() {
    assert_eq!(
        validate_strategy_id_for_path("../escape"),
        Err(StrategyIdError::PathSeparator),
    );
}

#[test]
fn validator_rejects_embedded_forward_slash() {
    assert_eq!(
        validate_strategy_id_for_path("foo/bar"),
        Err(StrategyIdError::PathSeparator),
    );
}

#[test]
fn validator_rejects_backslash() {
    assert_eq!(
        validate_strategy_id_for_path("foo\\bar"),
        Err(StrategyIdError::PathSeparator),
    );
}

#[test]
fn validator_rejects_leading_dot() {
    assert_eq!(
        validate_strategy_id_for_path(".hidden"),
        Err(StrategyIdError::LeadingDot),
    );
}

#[test]
fn validator_rejects_nul_byte() {
    assert_eq!(
        validate_strategy_id_for_path("foo\0bar"),
        Err(StrategyIdError::NulByte),
    );
}

// ── Store-level rejection: store root stays untouched ─────────────────

#[tokio::test]
async fn store_save_rejects_traversal_and_leaves_root_empty() {
    let parent = tempdir().unwrap();
    let root = parent.path().join("strategies");
    std::fs::create_dir_all(&root).unwrap();
    let store = FilesystemStore::new(root.clone());
    let strategy = strategy_with_id("../escape");

    let err = store.save(&strategy).await.unwrap_err();
    assert!(
        err.downcast_ref::<StrategyIdError>().is_some(),
        "expected StrategyIdError, got {err:?}",
    );

    // No file under the store root, and no `escape.json` written one
    // level up (which is where the traversal would have aimed had the
    // validation slipped).
    assert_store_root_unchanged(&root, &[]);
    let parent_bait = parent.path().join("escape.json");
    assert!(
        !parent_bait.exists(),
        "traversal target was created: {}",
        parent_bait.display()
    );
}

#[tokio::test]
async fn store_load_rejects_traversal_without_touching_disk() {
    // Plant the store root inside a dedicated parent dir so the "bait"
    // file the traversal would target lives in a fixture-private dir
    // (tests run in parallel and the system tempdir parent is shared).
    let parent = tempdir().unwrap();
    let root = parent.path().join("strategies");
    std::fs::create_dir_all(&root).unwrap();
    let store = FilesystemStore::new(root.clone());

    let bait = parent.path().join("escape.json");
    std::fs::write(&bait, b"{\"loot\":true}").unwrap();

    let err = store.load("../escape").await.unwrap_err();
    assert!(
        err.downcast_ref::<StrategyIdError>().is_some(),
        "expected StrategyIdError, got {err:?}",
    );

    let still_there = std::fs::read_to_string(&bait).unwrap();
    assert_eq!(still_there, "{\"loot\":true}");
}

#[tokio::test]
async fn store_delete_rejects_traversal_without_touching_disk() {
    let parent = tempdir().unwrap();
    let root = parent.path().join("strategies");
    std::fs::create_dir_all(&root).unwrap();
    let store = FilesystemStore::new(root);

    let bait = parent.path().join("escape.json");
    std::fs::write(&bait, b"{}").unwrap();

    let err = store.delete("../escape").await.unwrap_err();
    assert!(
        err.downcast_ref::<StrategyIdError>().is_some(),
        "expected StrategyIdError, got {err:?}",
    );
    assert!(bait.exists(), "bait file was unlinked by traversal!");
}

#[tokio::test]
async fn store_save_rejects_empty_id() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path().to_path_buf());
    let strategy = strategy_with_id("");
    let err = store.save(&strategy).await.unwrap_err();
    let kind = err.downcast_ref::<StrategyIdError>();
    assert_eq!(kind, Some(&StrategyIdError::Empty));
}

#[tokio::test]
async fn store_save_rejects_backslash() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path().to_path_buf());
    let strategy = strategy_with_id("foo\\bar");
    let err = store.save(&strategy).await.unwrap_err();
    let kind = err.downcast_ref::<StrategyIdError>();
    assert_eq!(kind, Some(&StrategyIdError::PathSeparator));
    assert_store_root_unchanged(dir.path(), &[]);
}

#[tokio::test]
async fn store_save_rejects_leading_dot() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path().to_path_buf());
    let strategy = strategy_with_id(".hidden");
    let err = store.save(&strategy).await.unwrap_err();
    let kind = err.downcast_ref::<StrategyIdError>();
    assert_eq!(kind, Some(&StrategyIdError::LeadingDot));
    assert_store_root_unchanged(dir.path(), &[]);
}

// ── Happy path still works ────────────────────────────────────────────

#[tokio::test]
async fn ulid_shaped_id_round_trips_through_store() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path().to_path_buf());
    let strategy = strategy_with_id("01HZSTRATEGY00000000000000");
    store.save(&strategy).await.unwrap();
    let loaded = store.load("01HZSTRATEGY00000000000000").await.unwrap();
    assert_eq!(loaded.manifest.id, "01HZSTRATEGY00000000000000");
    store.delete("01HZSTRATEGY00000000000000").await.unwrap();
}
