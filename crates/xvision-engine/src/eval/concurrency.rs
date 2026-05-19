//! Per-`(provider, model)` launch-concurrency gate for eval runs.
//!
//! Cap rationale: 2026-05-19 audit (`team/intake/2026-05-16-eval-review-and-v2a.md`,
//! F-1) — 27 eval runs launched against `google/gemini-3.1-flash-lite` in 15 s
//! burned ~18.3 M tokens and tripped OpenRouter's 450 RPM ceiling. The
//! dashboard's `start_run` had no shoulder against burst-launch, so a single
//! operator clicking through scenarios could blow the upstream quota.
//!
//! This module owns the in-process semaphore each `start_run` checks before
//! handing its background task to `tokio::spawn`. The semaphore key is the
//! literal `(provider, model)` pair the trader slot resolved to (e.g.
//! `("openrouter", "google/gemini-3.1-flash-lite")`). Distinct keys never
//! block each other; the same key serializes through whatever permit count
//! is configured for it.
//!
//! Per-key permits resolve in this order:
//!   1. Explicit per-key override from the in-memory `HashMap` (future
//!      TOML/env extension).
//!   2. Global default from `XVN_EVAL_MAX_CONCURRENT_PER_MODEL` (read once
//!      at gate construction).
//!   3. Compile-time default of `4`.
//!
//! The gate does NOT retry 429s (F-2, PR #347 owns that) and does NOT
//! serialize the finalize-write path (deferred; tracked in the F-1 PR body).
//! Its sole job is to bound how many in-flight runs share an upstream
//! model bucket.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

/// Compile-time default per-(provider, model) permit count.
///
/// Picked to give a comfortable shoulder against OpenRouter's lowest-tier
/// 450 RPM caps while still letting a few operators run evals in parallel.
/// Override at boot via `XVN_EVAL_MAX_CONCURRENT_PER_MODEL` or per-key via
/// `LaunchConcurrencyGate::with_overrides`.
pub const DEFAULT_PERMITS_PER_KEY: usize = 4;

/// Environment-variable name read by `LaunchConcurrencyGate::from_env`.
pub const ENV_MAX_CONCURRENT: &str = "XVN_EVAL_MAX_CONCURRENT_PER_MODEL";

/// In-process semaphore gate keyed by `(provider, model)`.
///
/// Cheap to clone (it's a couple of `Arc`s internally). Construct one per
/// process and stash it on `ApiContext`.
#[derive(Clone)]
pub struct LaunchConcurrencyGate {
    inner: Arc<GateInner>,
}

struct GateInner {
    default_permits: usize,
    overrides: HashMap<(String, String), usize>,
    semaphores: Mutex<HashMap<(String, String), Arc<Semaphore>>>,
}

/// RAII permit guard. The acquired slot is released when this value drops.
///
/// `start_run` moves the guard into the spawned background task so the
/// permit covers the entire run lifecycle — preflight, executor run,
/// finalize, post-process — and is freed even on panic.
#[must_use = "the permit is released when the guard is dropped; hold it for the full run"]
pub struct PermitGuard {
    _permit: OwnedSemaphorePermit,
    provider: String,
    model: String,
}

impl PermitGuard {
    /// Provider this guard is holding a slot against. Useful for logging.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Model this guard is holding a slot against. Useful for logging.
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl std::fmt::Debug for PermitGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermitGuard")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .finish()
    }
}

impl LaunchConcurrencyGate {
    /// Construct a gate with an explicit global default. Tests use this
    /// to pin a known permit count without touching the environment.
    pub fn new(default_permits: usize) -> Self {
        Self::with_overrides(default_permits, HashMap::new())
    }

    /// Construct a gate with both a global default and per-key overrides.
    ///
    /// Keys in `overrides` win over the global default. Future TOML wiring
    /// will populate this map from engine config; today it's seeded empty
    /// from `ApiContext::open`.
    pub fn with_overrides(default_permits: usize, overrides: HashMap<(String, String), usize>) -> Self {
        // A 0-permit cap would deadlock every acquire forever. Clamp to 1
        // and warn rather than letting the misconfig kill the engine.
        let default_permits = if default_permits == 0 {
            tracing::warn!(
                target: "xvision::eval::concurrency",
                "{ENV_MAX_CONCURRENT}=0 would deadlock; clamping to 1"
            );
            1
        } else {
            default_permits
        };
        let overrides = overrides
            .into_iter()
            .map(|(k, v)| (k, if v == 0 { 1 } else { v }))
            .collect();
        Self {
            inner: Arc::new(GateInner {
                default_permits,
                overrides,
                semaphores: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Construct a gate honoring `XVN_EVAL_MAX_CONCURRENT_PER_MODEL`. Falls
    /// back to `DEFAULT_PERMITS_PER_KEY` when the env var is unset, empty,
    /// or unparseable.
    pub fn from_env() -> Self {
        let default = std::env::var(ENV_MAX_CONCURRENT)
            .ok()
            .and_then(|raw| {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    trimmed.parse::<usize>().ok()
                }
            })
            .unwrap_or(DEFAULT_PERMITS_PER_KEY);
        Self::new(default)
    }

    /// Configured permit count for `(provider, model)`. Falls back to the
    /// gate's global default when there's no explicit override.
    pub fn permits_for(&self, provider: &str, model: &str) -> usize {
        self.inner
            .overrides
            .get(&(provider.to_string(), model.to_string()))
            .copied()
            .unwrap_or(self.inner.default_permits)
    }

    /// Acquire a permit for `(provider, model)`. Awaits until a permit is
    /// available; the returned guard MUST be moved into the spawned
    /// background task so the permit lives until the run finalizes.
    pub async fn acquire(&self, provider: &str, model: &str) -> PermitGuard {
        let key = (provider.to_string(), model.to_string());
        let sem = {
            let mut map = self.inner.semaphores.lock().await;
            map.entry(key.clone())
                .or_insert_with(|| {
                    let permits = self
                        .inner
                        .overrides
                        .get(&key)
                        .copied()
                        .unwrap_or(self.inner.default_permits);
                    Arc::new(Semaphore::new(permits))
                })
                .clone()
        };
        // Semaphore is never closed in our flow; `expect` documents that.
        let permit = sem
            .acquire_owned()
            .await
            .expect("launch concurrency semaphore is never closed");
        PermitGuard {
            _permit: permit,
            provider: provider.to_string(),
            model: model.to_string(),
        }
    }

    /// Number of permits currently available for `(provider, model)`.
    /// Test-surface only — production code should not depend on this.
    pub async fn available_permits(&self, provider: &str, model: &str) -> Option<usize> {
        let key = (provider.to_string(), model.to_string());
        let map = self.inner.semaphores.lock().await;
        map.get(&key).map(|sem| sem.available_permits())
    }
}

impl Default for LaunchConcurrencyGate {
    fn default() -> Self {
        Self::from_env()
    }
}

impl std::fmt::Debug for LaunchConcurrencyGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchConcurrencyGate")
            .field("default_permits", &self.inner.default_permits)
            .field("override_count", &self.inner.overrides.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    #[tokio::test]
    async fn permits_two_admits_two_blocks_third() {
        let gate = LaunchConcurrencyGate::new(2);

        let g1 = gate.acquire("openrouter", "gemini-3.1-flash-lite").await;
        let g2 = gate.acquire("openrouter", "gemini-3.1-flash-lite").await;

        // Third acquire must block.
        let gate_clone = gate.clone();
        let blocked =
            tokio::spawn(async move { gate_clone.acquire("openrouter", "gemini-3.1-flash-lite").await });

        // Give the spawned task a chance to actually try to acquire.
        sleep(Duration::from_millis(50)).await;
        assert!(!blocked.is_finished(), "third acquire must block");

        // Release one and the third must complete.
        drop(g1);
        let g3 = timeout(Duration::from_secs(2), blocked)
            .await
            .expect("third acquire should complete once a permit is freed")
            .expect("task should not panic");

        // Drop in test-scoped order so the compiler sees them used.
        drop(g2);
        drop(g3);
    }

    #[tokio::test]
    async fn distinct_keys_do_not_block_each_other() {
        let gate = LaunchConcurrencyGate::new(1);

        // Fill openrouter/gemini.
        let _g1 = gate.acquire("openrouter", "gemini-3.1-flash-lite").await;

        // Different model under same provider must NOT block.
        let g2 = timeout(
            Duration::from_millis(200),
            gate.acquire("openrouter", "gpt-5-codex"),
        )
        .await
        .expect("distinct model must not block on a saturated peer key");

        // Different provider under same model must NOT block either.
        let g3 = timeout(
            Duration::from_millis(200),
            gate.acquire("anthropic", "gemini-3.1-flash-lite"),
        )
        .await
        .expect("distinct provider must not block on a saturated peer key");

        drop(g2);
        drop(g3);
    }

    #[tokio::test]
    async fn release_admits_waiting_caller() {
        let gate = LaunchConcurrencyGate::new(1);

        let g1 = gate.acquire("p", "m").await;

        let gate_clone = gate.clone();
        let waiter = tokio::spawn(async move { gate_clone.acquire("p", "m").await });

        sleep(Duration::from_millis(50)).await;
        assert!(!waiter.is_finished(), "waiter must block while permit held");

        drop(g1);

        let g2 = timeout(Duration::from_secs(2), waiter)
            .await
            .expect("waiter should unblock when permit is dropped")
            .expect("task should not panic");
        drop(g2);
    }

    #[tokio::test]
    async fn per_key_override_wins_over_global_default() {
        let mut overrides = HashMap::new();
        overrides.insert(("openrouter".to_string(), "expensive-model".to_string()), 1);
        let gate = LaunchConcurrencyGate::with_overrides(4, overrides);

        assert_eq!(gate.permits_for("openrouter", "expensive-model"), 1);
        assert_eq!(gate.permits_for("openrouter", "cheap-model"), 4);

        // The override actually serializes: holding one slot must block a second.
        let _hold = gate.acquire("openrouter", "expensive-model").await;
        let blocked = tokio::spawn({
            let gate = gate.clone();
            async move { gate.acquire("openrouter", "expensive-model").await }
        });
        sleep(Duration::from_millis(50)).await;
        assert!(
            !blocked.is_finished(),
            "per-key override of 1 must serialize a second acquire"
        );
        blocked.abort();
    }

    #[tokio::test]
    async fn zero_permits_clamped_to_one_rather_than_deadlocking() {
        let gate = LaunchConcurrencyGate::new(0);
        // Must succeed (not hang) because we clamp 0 → 1.
        let _g = timeout(Duration::from_millis(200), gate.acquire("p", "m"))
            .await
            .expect("acquire on clamped gate must succeed");
    }

    #[tokio::test]
    async fn ten_acquires_with_permits_four_never_exceeds_four_in_flight() {
        let gate = Arc::new(LaunchConcurrencyGate::new(4));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..10 {
            let gate = gate.clone();
            let in_flight = in_flight.clone();
            let peak = peak.clone();
            handles.push(tokio::spawn(async move {
                let _g = gate.acquire("openrouter", "gemini-3.1-flash-lite").await;
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                // Simulate the run body.
                sleep(Duration::from_millis(20)).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(
            peak.load(Ordering::SeqCst) <= 4,
            "peak in-flight was {}, expected <=4",
            peak.load(Ordering::SeqCst)
        );
    }
}
