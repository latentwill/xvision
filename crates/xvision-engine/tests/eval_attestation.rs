//! Tests for the Phase 3.C signed-attestation surface.

#![allow(deprecated)] // canonical_scenarios() — see Task 8 (M2) deprecation note.

use chrono::{TimeZone, Utc};
use ed25519_dalek::SigningKey;
use sqlx::SqlitePool;
use xvision_engine::eval::attestation::{sign, verify, EvalAttestation};
use xvision_engine::eval::{canonical_scenarios, MetricsSummary, Run, RunMode, RunStore, Scenario};

async fn pool_with_migration() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

fn finalized_run() -> Run {
    // Returns a run in `queued` status. Callers that need to persist it
    // must `store.create(&run).await; store.begin_running(&run.id).await;
    // store.finalize(&run.id, &metrics).await;` — pre-setting
    // `status = Completed` here would make `store.finalize` (post #325)
    // bail with "run is already completed".
    let mut r = Run::new_queued(
        "strategy-hash-x".into(),
        "crypto-bull-q1-2025".into(),
        RunMode::Backtest,
    );
    r.completed_at = Some(Utc.with_ymd_and_hms(2025, 4, 1, 12, 0, 0).unwrap());
    r.actual_input_tokens = Some(12_345);
    r.actual_output_tokens = Some(6_789);
    r.metrics = Some(MetricsSummary {
        total_return_pct: 12.5,
        sharpe: 1.42,
        max_drawdown_pct: 8.3,
        win_rate: 0.58,
        n_trades: 17,
        n_decisions: 42,
        baselines: None,
    });
    r
}

fn first_scenario() -> Scenario {
    canonical_scenarios()[0].clone()
}

/// Deterministic test key. Tests don't need cryptographic randomness — a
/// fixed seed is reproducible and avoids pulling in `rand_core` as a
/// dev-dep.
fn fresh_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[
        0xa1, 0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x07, 0x18, 0x29, 0x3a, 0x4b, 0x5c, 0x6d, 0x7e, 0x8f, 0x90, 0xa1,
        0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x07, 0x18, 0x29, 0x3a, 0x4b, 0x5c, 0x6d, 0x7e, 0x8f, 0x90,
    ])
}

/// Distinct deterministic test key for "unrelated key" tests.
fn other_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11,
        0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
    ])
}

#[test]
fn sign_then_verify_round_trips() {
    let run = finalized_run();
    let scenario = first_scenario();
    let key = fresh_signing_key();
    let att = sign(&run, &scenario, &key).expect("sign must succeed");
    verify(&att).expect("attestation must verify");
}

#[test]
fn sign_carries_run_and_scenario_metadata() {
    let run = finalized_run();
    let scenario = first_scenario();
    let key = fresh_signing_key();
    let att = sign(&run, &scenario, &key).unwrap();
    assert_eq!(att.agent_id, run.agent_id);
    assert_eq!(att.scenario_id, scenario.id);
    assert_eq!(att.metrics, run.metrics.clone().unwrap());
    assert_eq!(att.tokens_used.input, 12_345);
    assert_eq!(att.tokens_used.output, 6_789);
    assert_eq!(att.tokens_used.total, 12_345 + 6_789);
    assert!(!att.signature_hex.is_empty());
    assert!(!att.signing_pubkey_hex.is_empty());
}

#[test]
fn sign_requires_finalized_metrics() {
    // A Run that hasn't called RunStore::finalize has metrics: None and
    // can't be attested. The function MUST refuse rather than emit a
    // garbage attestation.
    let mut run = finalized_run();
    run.metrics = None;
    let scenario = first_scenario();
    let key = fresh_signing_key();
    let err = sign(&run, &scenario, &key).expect_err("un-finalized run must error");
    assert!(
        err.to_string().contains("metrics"),
        "error should explain missing metrics, got {err}",
    );
}

#[test]
fn verify_fails_on_tampered_metrics() {
    let run = finalized_run();
    let scenario = first_scenario();
    let key = fresh_signing_key();
    let mut att = sign(&run, &scenario, &key).unwrap();
    // Mutate the signed metrics — verification must reject.
    att.metrics.total_return_pct = 999.9;
    let err = verify(&att).expect_err("tampered metrics must fail verify");
    assert!(
        err.to_string().to_lowercase().contains("verif")
            || err.to_string().to_lowercase().contains("signature"),
        "error should be a signature-verification failure, got {err}",
    );
}

fn assert_verify_rejects(att: &EvalAttestation, field: &str) {
    assert!(verify(att).is_err(), "{field} tampering must fail verify");
}

#[test]
fn verify_fails_on_tampered_signed_metadata() {
    let run = finalized_run();
    let scenario = first_scenario();
    let key = fresh_signing_key();
    let att = sign(&run, &scenario, &key).unwrap();

    let mut tampered = att.clone();
    tampered.agent_id = "tampered-agent".into();
    assert_verify_rejects(&tampered, "agent_id");

    let mut tampered = att.clone();
    tampered.scenario_id = "tampered-scenario".into();
    assert_verify_rejects(&tampered, "scenario_id");

    let mut tampered = att.clone();
    tampered.tokens_used.input += 1;
    assert_verify_rejects(&tampered, "tokens_used.input");

    let mut tampered = att.clone();
    tampered.tokens_used.output += 1;
    assert_verify_rejects(&tampered, "tokens_used.output");

    let mut tampered = att.clone();
    tampered.tokens_used.total += 1;
    assert_verify_rejects(&tampered, "tokens_used.total");

    let mut tampered = att.clone();
    tampered.ran_at = Utc.with_ymd_and_hms(2025, 4, 1, 12, 0, 1).unwrap();
    assert_verify_rejects(&tampered, "ran_at");
}

#[test]
fn verify_fails_on_tampered_pubkey() {
    let run = finalized_run();
    let scenario = first_scenario();
    let key = fresh_signing_key();
    let mut att = sign(&run, &scenario, &key).unwrap();
    // Replace pubkey with a fresh, unrelated one — the signature won't validate.
    let other_key = other_signing_key();
    att.signing_pubkey_hex = hex::encode(other_key.verifying_key().as_bytes());
    let err = verify(&att).expect_err("mismatched pubkey must fail verify");
    let _ = err; // any verification error is acceptable
}

#[test]
fn signing_same_payload_is_deterministic() {
    // Two attestations of the same run/scenario must produce identical
    // signatures when signed by the same key.
    let run = finalized_run();
    let scenario = first_scenario();
    let key = fresh_signing_key();

    let att1 = sign(&run, &scenario, &key).unwrap();
    let att2 = sign(&run, &scenario, &key).unwrap();
    assert_eq!(
        att1.signature_hex, att2.signature_hex,
        "canonical signing must be deterministic",
    );
}

#[tokio::test]
async fn run_store_record_attestation_and_get_round_trips() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);

    // Persist the run first so the FK is satisfied. Walk through legal
    // state transitions (Queued → Running via begin_running → Completed
    // via finalize) — `finalize` post #325 refuses pre-Completed rows.
    let mut run = finalized_run();
    let id = run.id.clone();
    store.create(&run).await.unwrap();
    store.begin_running(&id).await.unwrap();
    store.finalize(&id, run.metrics.as_ref().unwrap()).await.unwrap();
    run = store.get(&id).await.unwrap();

    let scenario = first_scenario();
    let key = fresh_signing_key();
    let att = sign(&run, &scenario, &key).unwrap();

    store
        .record_attestation(&run.id, &att)
        .await
        .expect("record must succeed");
    let read = store
        .get_attestation(&run.id)
        .await
        .expect("get must succeed")
        .expect("attestation must be present");
    assert_eq!(read.agent_id, att.agent_id);
    assert_eq!(read.scenario_id, att.scenario_id);
    assert_eq!(read.signature_hex, att.signature_hex);
    assert_eq!(read.signing_pubkey_hex, att.signing_pubkey_hex);
    // Verify the round-tripped attestation still validates.
    verify(&read).expect("round-tripped attestation must verify");
}

#[tokio::test]
async fn run_store_get_attestation_returns_none_when_missing() {
    let pool = pool_with_migration().await;
    let store = RunStore::new(pool);
    let mut run = finalized_run();
    store.create(&run).await.unwrap();
    store.begin_running(&run.id).await.unwrap();
    store
        .finalize(&run.id, run.metrics.as_ref().unwrap())
        .await
        .unwrap();
    run = store.get(&run.id).await.unwrap();
    let read = store.get_attestation(&run.id).await.unwrap();
    assert!(read.is_none());
}

#[test]
fn eval_attestation_json_round_trips() {
    let run = finalized_run();
    let scenario = first_scenario();
    let key = fresh_signing_key();
    let att = sign(&run, &scenario, &key).unwrap();
    let json = serde_json::to_string(&att).unwrap();
    let back: EvalAttestation = serde_json::from_str(&json).unwrap();
    assert_eq!(back.signature_hex, att.signature_hex);
    verify(&back).unwrap();
}
