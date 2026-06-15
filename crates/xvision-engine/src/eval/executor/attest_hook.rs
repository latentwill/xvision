//! Attest sink for the live executor (LANE byu — 20-trade auto-attest loop,
//! bead `xvision-byu`).
//!
//! The live executor counts executed (filled) trades and, every `N` trades
//! (default 20), invokes an [`AttestHook`] with a snapshot of the run's
//! listed performance. A manual attest route already exists (PR #919); this
//! is the *automatic* periodic surface.
//!
//! ## Dependency inversion (no engine → identity Cargo edge)
//!
//! The concrete attestation implementation lives in `xvision-identity` and
//! talks to the on-chain `EvalAttestationRegistry`. Adding a hard
//! `xvision-engine -> xvision-identity` dependency would invert the crate
//! layering (identity already depends on engine types) and pull the whole
//! alloy/chain stack into every engine build.
//!
//! Instead the engine defines this trait with a [`NoopAttestHook`] default
//! and the live executor calls it through a trait object. The concrete
//! identity-backed implementation is injected from the **dashboard** layer
//! (which already depends on both `xvision-engine` and `xvision-identity`),
//! via [`crate::eval::executor::Executor::with_attest_hook`]. Engine builds
//! that never wire a hook get the no-op and pay nothing.
//!
//! ## Fire-and-forget
//!
//! The live loop holds its `LiveRuntime` mutex across `.await`s, so the hook
//! MUST NOT block on a chain RPC inline. The executor spawns the hook call
//! (or the impl returns immediately and does its own background submit) so a
//! slow / failing attestation never stalls — or aborts — the trading loop.

use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::observability::ObsEmitter;

/// Snapshot of a live run's *listed performance* at an N-trade boundary,
/// handed to the [`AttestHook`]. Built from the same live accumulators that
/// feed `compute_run_metrics`, so the numbers an attestation publishes match
/// the metrics the run finalizes with.
///
/// Field names are developer-surface and stable: an injected identity impl
/// maps these onto the on-chain attestation payload.
#[derive(Debug, Clone)]
pub struct AttestSummary {
    /// The run this attestation is for (`eval_runs.id`, a ULID).
    pub run_id: String,
    /// The strategy / agent id (`Strategy.manifest.id`; becomes the NFT token
    /// id post-mint). Carried so the attestation targets the right listing.
    pub agent_id: String,
    /// Cumulative FILL LEGS that have crossed the book so far in this run —
    /// the same leg-count semantics as `MetricsSummary::n_trades`. This is
    /// always a positive multiple of the configured `every_n` at call time.
    pub n_trades: u32,
    /// Cumulative LLM-pipeline decisions so far.
    pub n_decisions: u32,
    /// Closed round-trips so far (the `win_rate` denominator).
    pub realized_count: u32,
    /// Round-trips that realized positive PnL so far (the `win_rate`
    /// numerator).
    pub wins: u32,
    /// Gross return as a percentage of starting capital, computed from the
    /// live equity curve at this boundary (`(equity - initial) / initial`).
    pub gross_return_pct: f64,
    /// Current pooled equity (NAV) in quote currency at this boundary.
    pub equity: f64,
}

/// Engine-side attest sink. The live executor calls [`Self::maybe_attest`]
/// once per `N` executed trades. The default ([`NoopAttestHook`]) does
/// nothing; the dashboard injects an identity-backed implementation.
///
/// Implementations MUST be fire-and-forget: the live loop awaits this call
/// while holding the runtime mutex, so a slow chain RPC inline would stall
/// trading. Return promptly (spawn your own background submit) and never
/// panic — a failed attestation must not abort the run.
///
/// ## WS-9 observability seam
///
/// The engine itself emits an `attest_boundary_reached` engine event at the
/// call site each time this fires (so the trace dock shows the boundary even
/// with the no-op hook). The DOWNSTREAM attestation-lifecycle events —
/// `attest_verdict`, `chain_submit_started`, `chain_submit_finished`,
/// `attestation_posted` — are the HOOK's to emit, because only the
/// identity-backed impl knows the verdict numbers, tx hash, gas, and registry
/// addresses. To let a future hook stream those onto the SAME observability
/// bus the engine already publishes onto, the executor threads its
/// `Option<ObsEmitter>` into this call. The hook calls
/// [`ObsEmitter::emit_engine_event`] with the same string-kind convention.
///
/// REDACTION CONTRACT: payloads a hook emits through `obs` carry tx hash,
/// contract/registry addresses, chain id, gas, block, and verdict numbers —
/// NEVER a private key or a raw signature. Do not put them in the payload in
/// the first place.
#[async_trait]
pub trait AttestHook: Send + Sync {
    /// Attest the listed performance captured in `summary`. Invoked exactly
    /// once each time the cumulative trade count crosses an `N`-trade
    /// boundary (`n_trades == N, 2N, 3N, …`).
    ///
    /// `obs` is the live run's [`ObsEmitter`] (cheap `Arc`-backed clone), or
    /// `None` for non-observed runs. A hook that performs an attestation
    /// should emit `attest_verdict` / `chain_submit_started` /
    /// `chain_submit_finished` / `attestation_posted` engine events through it
    /// so the on-chain submission is visible in the trace dock + run export.
    /// The no-op default ignores it.
    async fn maybe_attest(&self, summary: AttestSummary, obs: Option<ObsEmitter>);
}

/// The default no-op hook. Used by every engine path that does not inject a
/// concrete attestation impl (all unit tests, the CLI, backtests). Calling
/// it is a constant-time no-op.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopAttestHook;

#[async_trait]
impl AttestHook for NoopAttestHook {
    async fn maybe_attest(&self, _summary: AttestSummary, _obs: Option<ObsEmitter>) {}
}

// Blanket impl so an `Arc<H>` is itself an `AttestHook` — lets callers keep a
// clone of the concrete hook (e.g. to read it back in a test) while handing
// the executor an `Arc<dyn AttestHook>`.
#[async_trait]
impl<H: AttestHook + ?Sized> AttestHook for Arc<H> {
    async fn maybe_attest(&self, summary: AttestSummary, obs: Option<ObsEmitter>) {
        (**self).maybe_attest(summary, obs).await;
    }
}

/// Clamp the configured trade interval to at least 1 so the boundary modulo
/// can never divide by zero. A caller passing `0` (or a defaulted field) is
/// treated as "every trade".
#[inline]
pub fn clamp_every_n(every_n: u32) -> u32 {
    every_n.max(1)
}

/// True when a cumulative trade count `n_trades` lands exactly on an
/// `every_n`-trade boundary. The pure decision the live loop uses after each
/// fill: fire at `every_n`, `2*every_n`, … and never between.
///
/// `every_n` is clamped to at least 1 (so a zero interval fires every trade
/// rather than panicking on `% 0`). `n_trades == 0` never fires (no trade has
/// executed yet).
#[inline]
pub fn is_attest_boundary(n_trades: u32, every_n: u32) -> bool {
    if n_trades == 0 {
        return false;
    }
    n_trades % clamp_every_n(every_n) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn boundary_fires_at_multiples_of_n_and_not_between() {
        // N = 20: fires at 20, 40, 60; not at 0/19/21/39/41/59.
        assert!(!is_attest_boundary(0, 20), "no trades yet => no attest");
        assert!(!is_attest_boundary(19, 20), "19 < 20 => no attest");
        assert!(is_attest_boundary(20, 20), "exactly 20 => attest");
        assert!(!is_attest_boundary(21, 20), "21 just past 20 => no attest");
        assert!(!is_attest_boundary(39, 20), "39 < 40 => no attest");
        assert!(is_attest_boundary(40, 20), "exactly 40 => attest");
        assert!(!is_attest_boundary(41, 20), "41 just past 40 => no attest");
        assert!(is_attest_boundary(60, 20), "exactly 60 => attest");
    }

    #[test]
    fn every_n_is_clamped_to_at_least_one() {
        assert_eq!(clamp_every_n(0), 1, "0 clamps to 1 (avoids % 0 panic)");
        assert_eq!(clamp_every_n(1), 1);
        assert_eq!(clamp_every_n(20), 20);
        // With the clamp, a zero interval fires on every trade rather than
        // dividing by zero.
        assert!(is_attest_boundary(1, 0), "clamped N=1 fires every trade");
        assert!(is_attest_boundary(7, 0));
        assert!(!is_attest_boundary(0, 0), "still no fire at zero trades");
    }

    #[tokio::test]
    async fn noop_hook_is_inert() {
        // The default never records or panics.
        let hook = NoopAttestHook;
        hook.maybe_attest(
            AttestSummary {
                run_id: "R".into(),
                agent_id: "A".into(),
                n_trades: 20,
                n_decisions: 25,
                realized_count: 10,
                wins: 6,
                gross_return_pct: 1.5,
                equity: 101_500.0,
            },
            None,
        )
        .await;
    }

    #[tokio::test]
    async fn arc_blanket_impl_forwards_to_inner() {
        #[derive(Default)]
        struct Recorder {
            seen: Mutex<Vec<u32>>,
        }
        #[async_trait]
        impl AttestHook for Recorder {
            async fn maybe_attest(&self, summary: AttestSummary, _obs: Option<ObsEmitter>) {
                self.seen.lock().unwrap().push(summary.n_trades);
            }
        }
        let rec = Arc::new(Recorder::default());
        // Drive through the Arc blanket impl.
        let as_hook: Arc<dyn AttestHook> = rec.clone();
        as_hook
            .maybe_attest(
                AttestSummary {
                    run_id: "R".into(),
                    agent_id: "A".into(),
                    n_trades: 40,
                    n_decisions: 50,
                    realized_count: 20,
                    wins: 12,
                    gross_return_pct: 2.0,
                    equity: 102_000.0,
                },
                None,
            )
            .await;
        assert_eq!(rec.seen.lock().unwrap().clone(), vec![40]);
    }
}
