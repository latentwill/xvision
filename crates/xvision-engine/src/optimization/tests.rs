//! Store round-trip + reproducibility tests for the Phase 3.5 optimization store.
//!
//! No xvision-dspy dependency: these tests treat the snapshot/demos as opaque
//! JSON strings, exactly as the engine does at runtime.

use sqlx::SqlitePool;

use super::*;

/// Migration 045 split exactly as `ApiContext::open`'s `migrate_optimization_store`
/// does, so the test exercises the same DDL the runtime applies.
const MIGRATION_045: &str = include_str!("../../migrations/045_optimization_store.sql");

/// Strip `--` comments then split on `;` — mirrors the runtime
/// `migrate_optimization_store` statement splitter so the test exercises the
/// same DDL path.
fn split_statements(sql: &str) -> Vec<String> {
    let without_comments: String = sql
        .lines()
        .map(|line| match line.find("--") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");
    without_comments
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Build a fresh in-memory pool with migration 045 applied (statement-split).
async fn fresh_store() -> OptimizationStore {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    for stmt in split_statements(MIGRATION_045) {
        sqlx::query(&stmt).execute(&pool).await.unwrap();
    }
    OptimizationStore::new(pool)
}

fn sample_run() -> NewOptimizationRun {
    NewOptimizationRun {
        agent_id: "01AGENTPARENT".to_string(),
        slot_name: "trader".to_string(),
        capability: "trader".to_string(),
        optimizer: "mipro".to_string(),
        metric: "delta_sharpe".to_string(),
        corpus_query: "scenario:bull-2024 limit=200".to_string(),
        rng_seed: 42,
        model_provider: Some("dummy".to_string()),
        model_name: Some("dummy".to_string()),
        signature_hash: Some("abc123sighash".to_string()),
        optimizer_version: Some("dspy-rs-0.7.3".to_string()),
    }
}

#[tokio::test]
async fn migration_045_fresh_db_creates_all_tables_and_indexes() {
    let store = fresh_store().await;
    let pool = &store.pool;
    for table in [
        "optimization_runs",
        "optimization_candidates",
        "optimization_demos",
        "optimization_snapshots",
        "agent_lineage",
    ] {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(table)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(count.0, 1, "table {table} missing after migration 045");
    }
    for index in [
        "idx_optimization_runs_agent",
        "idx_optimization_candidates_run",
        "idx_optimization_snapshots_run",
        "idx_agent_lineage_parent",
    ] {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?",
        )
        .bind(index)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(count.0, 1, "index {index} missing after migration 045");
    }
}

#[tokio::test]
async fn run_round_trips_through_store() {
    let store = fresh_store().await;
    let created = store.create_run(sample_run()).await.unwrap();
    assert_eq!(created.status, "pending");
    assert!(!created.id.is_empty());

    let fetched = store.get_run(&created.id).await.unwrap();
    assert_eq!(created, fetched);

    // status transition persists
    store.set_run_status(&created.id, "completed").await.unwrap();
    let after = store.get_run(&created.id).await.unwrap();
    assert_eq!(after.status, "completed");

    // listed by agent
    let listed = store
        .list_runs_for_agent(&created.agent_id, Some("trader"))
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
}

#[tokio::test]
async fn candidates_round_trip_and_order_by_index() {
    let store = fresh_store().await;
    let run = store.create_run(sample_run()).await.unwrap();

    // insert out of order; list must come back ordered by candidate_index
    for idx in [2_i64, 0, 1] {
        store
            .add_candidate(
                &run.id,
                NewCandidate {
                    candidate_index: idx,
                    instruction: format!("candidate instruction {idx}"),
                    metric_value: Some(idx as f64 * 0.1),
                    split: "train".to_string(),
                    demo_set: None,
                    selected: idx == 1,
                },
            )
            .await
            .unwrap();
    }
    let cands = store.list_candidates(&run.id).await.unwrap();
    assert_eq!(cands.len(), 3);
    assert_eq!(
        cands.iter().map(|c| c.candidate_index).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(cands[1].selected);
    assert!(!cands[0].selected);

    // re-select a different winner: exactly one selected, the rest cleared.
    store.mark_candidate_selected(&run.id, 2).await.unwrap();
    let cands = store.list_candidates(&run.id).await.unwrap();
    let selected: Vec<_> = cands.iter().filter(|c| c.selected).collect();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].candidate_index, 2);

    // selecting a non-existent index is NotFound.
    assert!(matches!(
        store.mark_candidate_selected(&run.id, 99).await,
        Err(ApiError::NotFound(_))
    ));
}

#[tokio::test]
async fn demo_sets_are_content_addressed_and_deduplicated() {
    let store = fresh_store().await;
    let payload = r#"[{"inputs":{"briefing":"x"},"outputs":{"action":"buy"}}]"#;

    let h1 = store.put_demo_set(payload).await.unwrap();
    let h2 = store.put_demo_set(payload).await.unwrap();
    assert_eq!(h1, h2, "identical payloads must share a content hash");
    assert_eq!(h1, demo_set_hash(payload));

    // re-storing identical payload is a no-op: still exactly one row
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM optimization_demos")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);

    let fetched = store.get_demo_set(&h1).await.unwrap();
    assert_eq!(fetched, payload);

    // a different payload gets a different hash + its own row
    let other = r#"[{"inputs":{"briefing":"y"},"outputs":{"action":"sell"}}]"#;
    let h3 = store.put_demo_set(other).await.unwrap();
    assert_ne!(h1, h3);
}

#[tokio::test]
async fn snapshot_round_trips_and_accept_flag_toggles() {
    let store = fresh_store().await;
    let run = store.create_run(sample_run()).await.unwrap();

    // an opaque snapshot JSON — engine never parses this
    let snapshot_json = r#"{"id":"01SNAP","instruction":"optimized prompt","demos":[],"signature_hash":"abc123sighash","metric_name":"delta_sharpe","corpus_query":"scenario:bull-2024 limit=200","rng_seed":42,"optimizer_name":"mipro","optimizer_version":"dspy-rs-0.7.3","parent_id":null,"child_ids":[]}"#;

    let snap = store
        .add_snapshot(
            &run.id,
            NewSnapshot {
                id: "01SNAP".to_string(),
                snapshot_json: snapshot_json.to_string(),
                signature_hash: "abc123sighash".to_string(),
                demo_set: None,
            },
        )
        .await
        .unwrap();
    assert!(!snap.accepted);

    let fetched = store.get_snapshot("01SNAP").await.unwrap();
    assert_eq!(snap, fetched);
    assert_eq!(fetched.snapshot_json, snapshot_json);

    store.set_snapshot_accepted("01SNAP", true).await.unwrap();
    let accepted = store.get_snapshot("01SNAP").await.unwrap();
    assert!(accepted.accepted);

    let listed = store.list_snapshots(&run.id).await.unwrap();
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn lineage_edge_round_trips_and_dedups() {
    let store = fresh_store().await;
    let run = store.create_run(sample_run()).await.unwrap();

    let edge = store
        .add_lineage("01CHILD", "01AGENTPARENT", &run.id)
        .await
        .unwrap();
    assert_eq!(edge.child_agent_id, "01CHILD");
    assert_eq!(edge.parent_agent_id, "01AGENTPARENT");

    let got = store.get_lineage_for_child("01CHILD").await.unwrap();
    assert_eq!(got, Some(edge.clone()));

    let children = store.list_children("01AGENTPARENT").await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].child_agent_id, "01CHILD");

    // duplicate child is a conflict
    let dup = store.add_lineage("01CHILD", "01AGENTPARENT", &run.id).await;
    assert!(matches!(dup, Err(ApiError::Conflict(_))));

    // revert removes the edge
    store.delete_lineage_for_child("01CHILD").await.unwrap();
    assert_eq!(store.get_lineage_for_child("01CHILD").await.unwrap(), None);
}

/// The reproducibility contract: a run can be reconstructed from its persisted
/// inputs alone. We persist a run + its accepted snapshot + demo set, then
/// rebuild the reproduction recipe purely from the store and assert it matches
/// the original inputs and the snapshot's reproducible fields.
#[tokio::test]
async fn run_is_reproducible_from_persisted_inputs() {
    let store = fresh_store().await;
    let req = sample_run();
    let run = store.create_run(req.clone()).await.unwrap();

    // persist the demo set the snapshot references
    let demos_json = r#"[{"inputs":{"briefing":"ctx"},"outputs":{"action":"hold","size_fraction":0.0,"rationale":"flat"}}]"#;
    let demo_set = store.put_demo_set(demos_json).await.unwrap();

    // persist the accepted snapshot (opaque blob carrying the repro fields)
    let snapshot_json = format!(
        r#"{{"id":"01SNAP","instruction":"optimized","demos":{demos_json},"signature_hash":"{sig}","metric_name":"{metric}","corpus_query":"{corpus}","rng_seed":{seed},"optimizer_name":"{opt}","optimizer_version":"{ver}","parent_id":null,"child_ids":[]}}"#,
        sig = req.signature_hash.clone().unwrap(),
        metric = req.metric,
        corpus = req.corpus_query,
        seed = req.rng_seed,
        opt = req.optimizer,
        ver = req.optimizer_version.clone().unwrap(),
    );
    store
        .add_snapshot(
            &run.id,
            NewSnapshot {
                id: "01SNAP".to_string(),
                snapshot_json: snapshot_json.clone(),
                signature_hash: req.signature_hash.clone().unwrap(),
                demo_set: Some(demo_set.clone()),
            },
        )
        .await
        .unwrap();
    store.set_snapshot_accepted("01SNAP", true).await.unwrap();

    // --- reconstruct from persisted inputs ALONE ---
    let recipe = store.reproduction_recipe(&run.id).await.unwrap();
    assert_eq!(recipe.corpus_query, req.corpus_query);
    assert_eq!(recipe.rng_seed, req.rng_seed);
    assert_eq!(recipe.model_provider, req.model_provider);
    assert_eq!(recipe.model_name, req.model_name);
    assert_eq!(recipe.optimizer, req.optimizer);
    assert_eq!(recipe.optimizer_version, req.optimizer_version);
    assert_eq!(recipe.signature_hash, req.signature_hash);
    assert_eq!(recipe.metric, req.metric);

    // the accepted snapshot is fetchable and its demo set is recoverable by hash
    let snap = store.get_snapshot("01SNAP").await.unwrap();
    assert!(snap.accepted);
    let recovered_demos = store.get_demo_set(&snap.demo_set.clone().unwrap()).await.unwrap();
    assert_eq!(recovered_demos, demos_json);

    // content-address integrity: the stored hash equals a fresh hash of the payload
    assert_eq!(snap.demo_set.unwrap(), demo_set_hash(demos_json));

    // the snapshot blob round-trips byte-for-byte (engine stores it opaque)
    assert_eq!(snap.snapshot_json, snapshot_json);
}

#[tokio::test]
async fn get_run_missing_is_not_found() {
    let store = fresh_store().await;
    let err = store.get_run("nope").await.unwrap_err();
    assert!(matches!(err, ApiError::NotFound(_)));
}

#[tokio::test]
async fn cascade_delete_run_removes_children() {
    let store = fresh_store().await;
    // foreign_keys pragma must be on for cascade; in-memory connect default is off,
    // so enable it explicitly to exercise the ON DELETE CASCADE.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&store.pool)
        .await
        .unwrap();

    let run = store.create_run(sample_run()).await.unwrap();
    store
        .add_candidate(
            &run.id,
            NewCandidate {
                candidate_index: 0,
                instruction: "c".to_string(),
                metric_value: None,
                split: "train".to_string(),
                demo_set: None,
                selected: false,
            },
        )
        .await
        .unwrap();
    store
        .add_snapshot(
            &run.id,
            NewSnapshot {
                id: "01SNAP".to_string(),
                snapshot_json: "{}".to_string(),
                signature_hash: "h".to_string(),
                demo_set: None,
            },
        )
        .await
        .unwrap();

    sqlx::query("DELETE FROM optimization_runs WHERE id = ?")
        .bind(&run.id)
        .execute(&store.pool)
        .await
        .unwrap();

    assert!(store.list_candidates(&run.id).await.unwrap().is_empty());
    assert!(store.list_snapshots(&run.id).await.unwrap().is_empty());
}
