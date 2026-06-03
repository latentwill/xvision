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
//!   - the production gated-spawn helper holds the permit until the
//!     background task exits,
//!   - permits configured via `XVN_EVAL_MAX_CONCURRENT_PER_MODEL` are
//!     respected by `ApiContext::new`'s default gate construction.
//!
//! The audit headline (27 launches → 18.3 M tokens / 450 RPM) is the
//! reason the gate exists; full run execution is covered by
//! `api_eval_run.rs`, while this file pins the launch-gate seam used by
//! `start_run`: acquire before spawn, release when the spawned task exits.

use std::ffi::OsString;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sqlx::sqlite::SqlitePoolOptions;
use tokio::time::{sleep, timeout};
use xvision_engine::api::{eval as api_eval, Actor, ApiContext};
use xvision_engine::eval::concurrency::{LaunchConcurrencyGate, ENV_MAX_CONCURRENT};

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct EnvVarGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(original) = &self.original {
            std::env::set_var(self.key, original);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

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
        ctx.launch_gate
            .acquire("openrouter", "google/gemini-3.1-flash-lite"),
    )
    .await
    .expect("default gate must admit a single acquire");
    drop(g);
}

#[tokio::test]
async fn api_context_default_launch_gate_honors_env_permit_count() {
    let _env_lock = ENV_LOCK.lock().await;
    let _env = EnvVarGuard::set(ENV_MAX_CONCURRENT, "2");

    let (ctx, _d) = empty_ctx().await;
    assert_eq!(
        ctx.launch_gate
            .permits_for("openrouter", "google/gemini-3.1-flash-lite"),
        2,
        "ApiContext::new must construct its default gate from {ENV_MAX_CONCURRENT}"
    );

    let first = ctx
        .launch_gate
        .acquire("openrouter", "google/gemini-3.1-flash-lite")
        .await;
    let second = ctx
        .launch_gate
        .acquire("openrouter", "google/gemini-3.1-flash-lite")
        .await;
    let third = tokio::spawn({
        let gate = ctx.launch_gate.clone();
        async move { gate.acquire("openrouter", "google/gemini-3.1-flash-lite").await }
    });

    sleep(Duration::from_millis(20)).await;
    assert!(
        !third.is_finished(),
        "third same-key acquire must block while env-configured two permits are held"
    );

    drop(first);
    let third = timeout(Duration::from_secs(2), third)
        .await
        .expect("third acquire must unblock once one env-configured permit is released")
        .expect("third acquire task should not panic");
    drop(second);
    drop(third);
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
    let _other_model = timeout(
        Duration::from_millis(200),
        ctx.launch_gate.acquire("openrouter", "openai/gpt-5-codex"),
    )
    .await
    .expect("distinct model must not block on a saturated peer key");

    let _other_provider = timeout(
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
            let permit = gate.acquire("openrouter", "google/gemini-3.1-flash-lite").await;
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
async fn start_run_gated_spawn_helper_holds_permit_until_task_exit() {
    let (ctx, _d) = empty_ctx().await;
    let ctx = ctx.with_launch_gate(Arc::new(LaunchConcurrencyGate::new(1)));

    let permit = ctx.launch_gate.acquire("p", "m").await;
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();

    let task = api_eval::spawn_launch_gated_task(permit, async move {
        let _ = started_tx.send(());
        let _ = release_rx.await;
    });
    started_rx.await.expect("gated task should start");

    let blocked = tokio::spawn({
        let gate = ctx.launch_gate.clone();
        async move { gate.acquire("p", "m").await }
    });
    sleep(Duration::from_millis(20)).await;
    assert!(
        !blocked.is_finished(),
        "second acquire must block while production gated-spawn helper owns the permit"
    );

    release_tx.send(()).unwrap();
    task.await.unwrap();

    let g = timeout(Duration::from_secs(2), blocked)
        .await
        .expect("blocked acquire must unblock after gated task exits")
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
