//! Integration coverage for the per-`(provider, model)` launch-concurrency
//! gate (`eval-launch-concurrency-cap`, F-1, 2026-05-19). The unit tests in
//! `crates/xvision-engine/src/eval/concurrency.rs` cover the semaphore
//! mechanics; this file exercises the gate through `ApiContext` to confirm:
//!
//!   - the gate is reachable from a real `ApiContext` (i.e. the field is
//!     wired and clones work),
//!   - distinct `(provider, model)` keys do not block each other,
//!   - the same key with permits=N never has more than N in-flight
//!     callers, even under contention,
//!   - permits configured via `XVN_EVAL_MAX_CONCURRENT_PER_MODEL` are
//!     respected (via the `with_launch_gate` builder, since reading the
//!     env in tests is racy across threads).
//!
//! The audit headline (27 launches → 18.3 M tokens / 450 RPM) is the
//! reason the gate exists; we don't reproduce a full `start_run` here
//! because that pulls broker + scenario + executor machinery already
//! covered by `api_eval_run.rs`. The contract for F-1 is "permit is
//! acquired before spawn, released when the spawned task exits", which is
//! a one-line lifetime assertion against the gate's `available_permits`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sqlx::sqlite::SqlitePoolOptions;
use tokio::time::{sleep, timeout};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::concurrency::LaunchConcurrencyGate;

async fn empty_ctx() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    (ctx, dir)
}

#[tokio::test]
async fn api_context_exposes_launch_gate_by_default() {
    let (ctx, _d) = empty_ctx().await;
    // Default gate should accept at least one acquire (permits >= 1 by
    // construction).
    let g = timeout(
        Duration::from_secs(1),
        ctx.launch_gate.acquire("openrouter", "google/gemini-3.1-flash-lite"),
    )
    .await
    .expect("default gate must admit a single acquire");
    drop(g);
}

#[tokio::test]
async fn distinct_provider_model_keys_run_in_parallel() {
    let (ctx, _d) = empty_ctx().await;
    let ctx = ctx.with_launch_gate(Arc::new(LaunchConcurrencyGate::new(1)));

    // Saturate one key.
    let _hold = ctx
        .launch_gate
        .acquire("openrouter", "google/gemini-3.1-flash-lite")
        .await;

    // A second key must not block, even with global permits=1.
    timeout(
        Duration::from_millis(200),
        ctx.launch_gate.acquire("openrouter", "openai/gpt-5-codex"),
    )
    .await
    .expect("distinct model must not block on a saturated peer key");

    timeout(
        Duration::from_millis(200),
        ctx.launch_gate
            .acquire("anthropic", "google/gemini-3.1-flash-lite"),
    )
    .await
    .expect("distinct provider must not block on a saturated peer key");
}

#[tokio::test]
async fn ten_acquires_against_same_key_cap_at_four_in_flight() {
    let (ctx, _d) = empty_ctx().await;
    let ctx = ctx.with_launch_gate(Arc::new(LaunchConcurrencyGate::new(4)));

    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    // Mirror the start_run shape: acquire, then move the guard into a
    // spawned task that simulates the executor.
    let mut handles = Vec::with_capacity(10);
    for _ in 0..10 {
        let gate = ctx.launch_gate.clone();
        let in_flight = in_flight.clone();
        let peak = peak.clone();
        let handle = tokio::spawn(async move {
            // Acquire on the foreground (as start_run does), then move
            // the guard into the simulated background task body.
            let permit = gate
                .acquire("openrouter", "google/gemini-3.1-flash-lite")
                .await;
            tokio::spawn(async move {
                let _permit = permit; // held for run lifetime
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                sleep(Duration::from_millis(30)).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
            })
            .await
            .unwrap();
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.unwrap();
    }

    assert!(
        peak.load(Ordering::SeqCst) <= 4,
        "peak in-flight was {}, expected <=4",
        peak.load(Ordering::SeqCst)
    );

    // After every spawned body has dropped its guard, all 4 permits
    // should be free again.
    let avail = ctx
        .launch_gate
        .available_permits("openrouter", "google/gemini-3.1-flash-lite")
        .await
        .expect("the key was used, so the semaphore should exist");
    assert_eq!(avail, 4, "all permits should be released, got {avail}");
}

#[tokio::test]
async fn permit_guard_held_in_spawned_task_releases_on_task_drop() {
    let (ctx, _d) = empty_ctx().await;
    let ctx = ctx.with_launch_gate(Arc::new(LaunchConcurrencyGate::new(1)));

    // First spawn mirrors start_run: acquire, then spawn.
    let gate = ctx.launch_gate.clone();
    let permit = gate.acquire("p", "m").await;
    let task = tokio::spawn(async move {
        let _permit = permit;
        sleep(Duration::from_millis(50)).await;
    });

    // While the spawned task holds the permit, a second foreground
    // acquire must block.
    let blocked = tokio::spawn({
        let gate = ctx.launch_gate.clone();
        async move { gate.acquire("p", "m").await }
    });
    sleep(Duration::from_millis(20)).await;
    assert!(
        !blocked.is_finished(),
        "second acquire must block while the spawned task still owns the permit"
    );

    // Let the spawned task finish — the permit drops with it.
    task.await.unwrap();

    // Now the blocked acquire must complete.
    let g = timeout(Duration::from_secs(2), blocked)
        .await
        .expect("blocked acquire must unblock once the spawned task drops the permit")
        .expect("task should not panic");
    drop(g);
}

#[tokio::test]
#[ignore = "stress: bumps permits to a realistic prod value and burns a few hundred ms"]
async fn stress_fifty_acquires_with_permits_eight_caps_at_eight() {
    let (ctx, _d) = empty_ctx().await;
    let ctx = ctx.with_launch_gate(Arc::new(LaunchConcurrencyGate::new(8)));

    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::with_capacity(50);
    for _ in 0..50 {
        let gate = ctx.launch_gate.clone();
        let in_flight = in_flight.clone();
        let peak = peak.clone();
        handles.push(tokio::spawn(async move {
            let _g = gate.acquire("p", "m").await;
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            sleep(Duration::from_millis(20)).await;
            in_flight.fetch_sub(1, Ordering::SeqCst);
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert!(peak.load(Ordering::SeqCst) <= 8);
}
