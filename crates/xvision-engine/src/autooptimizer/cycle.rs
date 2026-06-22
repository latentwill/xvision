//! Optimizer cycle orchestrator — AR-2 Task 9.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use ulid::Ulid;

use crate::autooptimizer::blob_store::BlobStore;
use crate::autooptimizer::canary::{run_honesty_check, HonestyCheckResult};
use crate::autooptimizer::config::{
    validate_regime_set, AutoOptimizerConfig, RegimeSide, RegimeWindow, TradeDirection,
};
use crate::autooptimizer::content_hash::ContentHash;
// [loosening-disabled 2026-06-22] preserved for later opt-in re-enablement
// use crate::autooptimizer::cycle_loosen::effective_min_improvement_for_cycle;
use crate::autooptimizer::diversity::diversity_decay_for_cycle;
use crate::autooptimizer::dspy_flywheel::{handle_cycle_dspy, query_dsr_prefix, DspyContext};
use crate::autooptimizer::eval_adapter::PaperTestRunner;
use crate::autooptimizer::evidence::{persist_finding, persist_gate_record, GateRecord};
use crate::autooptimizer::gate::{aggregate_regime_verdicts, evaluate, GateInput, GateVerdict, Objective};
use crate::autooptimizer::inversion::run_inversion_pair;
use crate::autooptimizer::judge::{run_judge, Finding, Judge};
use crate::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore};
use crate::autooptimizer::mutator::{MutationDiff, Mutator};
use crate::autooptimizer::mutator_ladder::{record_outcome, record_proposal};
use crate::autooptimizer::parent_policy::{select_parents, ParentPolicy};
use crate::autooptimizer::progress::{CycleProgressEvent, Phase};
use crate::autooptimizer::regime_results::{insert_regime_results, RegimeResultRow};
use crate::eval::run::MetricsSummary;
use crate::eval::scenario::Scenario;
use crate::strategies::Strategy;

/// Per-cycle configuration.
///
/// `Clone` so the CLI session loop can derive a fresh per-cycle config from one
/// base, varying only `sustained_no_pass_cycles` across cycles (GH #965).
#[derive(Clone)]
pub struct CycleConfig {
    pub num_parents: usize,
    pub mutations_per_parent: usize,
    pub sabotage_seed: u64,
    pub judge_provider: String,
    pub judge_model: String,
    pub prompt_version: String,
    /// Consecutive cycles with zero merges; drives threshold loosening.
    pub sustained_no_pass_cycles: u32,
    /// Day-window scenario for primary evaluation.
    pub day_scenario: Scenario,
    /// Held-out baseline-untouched scenario for overfitting guard.
    pub baseline_scenario: Scenario,
    /// bundle_hash hex → Strategy for seed parents selected this cycle.
    pub parent_strategies: HashMap<String, Strategy>,
    /// Optional explicit parent bundle hashes.
    pub explicit_parent_hashes: Vec<ContentHash>,
    /// F24: the metric this cycle optimizes (gate objective). Defaults to Sharpe.
    pub objective: Objective,
    /// Regime windows for the regime-matrix feature (Phase 2).
    /// When empty the orchestrator uses the single day+baseline path unchanged.
    /// Populated from `AutoOptimizerConfig.regime_set`.
    pub regime_set: Vec<RegimeWindow>,
    /// B19: pre-synthesized pool of `(day_scenario, baseline_scenario)` pairs the
    /// cycle SAMPLES round-robin across candidates (candidate `i` uses
    /// `scenario_pool[i % len]`), so different candidates are scored on different
    /// regimes and a strategy tuned to one fixed window can't dominate the cycle.
    ///
    /// Synthesized from `AutoOptimizerConfig.scenario_pool` (one entry per
    /// `ScenarioWindowPair`). EMPTY (the default) ⇒ every candidate uses the
    /// single `day_scenario`/`baseline_scenario` pair above, exactly as before
    /// (back-compat). Ignored on the regime-matrix path (`regime_set` non-empty),
    /// which has its own multi-window semantics.
    pub scenario_pool: Vec<(Scenario, Scenario)>,
    /// Strict per-call output-token cap applied to EVERY LLM dispatch this
    /// cycle (candidate paper-test trader decisions, the experiment writer,
    /// and the judge). `None` = no cycle-level cap; each slot keeps its own
    /// `max_tokens`. `Some(n)` is the cap the CLI installs via
    /// [`crate::autooptimizer::metering_dispatch::MaxTokensCapDispatch`] —
    /// enforcement happens at the provider boundary, so this field is the
    /// recorded cycle-level intent that travels with the config.
    ///
    /// Set by `xvn optimizer run-cycle --max-output-tokens N`.
    pub max_output_tokens: Option<u32>,
    /// Circuit-breaker limit: how many consecutive candidate eval failures halt
    /// the session with a loud error (systemic misconfiguration). `0` disables
    /// the breaker (never halts). Default: 3.
    ///
    /// WU-13 will wire --max-consecutive-errors from the CLI args.
    pub max_consecutive_errors: u32,
}

pub struct CycleResult {
    pub cycle_id: String,
    pub active_nodes: Vec<LineageNode>,
    /// Nodes that passed on some regimes but not all (Quarantined / "Suspect").
    /// These are distinct from `rejected_nodes` — they partially improved the
    /// strategy and deserve their own tier in summaries and CLI output.
    pub suspect_nodes: Vec<LineageNode>,
    pub rejected_nodes: Vec<LineageNode>,
    pub honesty_check: HonestyCheckResult,
    pub diversity_score: f64,
    pub findings_by_node: HashMap<ContentHash, Vec<Finding>>,
    /// Number of (parent × mutation) iterations that yielded no usable
    /// candidate — the experiment writer could not produce a distinct, valid
    /// experiment (e.g. only no-op/identity diffs). Lets the CLI/panel
    /// distinguish a genuinely empty cycle from one that gated a real candidate
    /// (F14, QA 2026-06-04).
    pub no_candidate_count: usize,
    /// Number of candidate eval errors that were caught and skipped (non-fatal)
    /// this cycle. Distinct from `no_candidate_count` (which tracks writer
    /// failures) — this tracks eval/backtest failures. Aggregated from
    /// `process_parent_mutations`. 2026-06-13 trader-failure resilience.
    pub errored_count: usize,
}

struct MutationOutcome {
    child: Strategy,
    diff: MutationDiff,
    child_hash: ContentHash,
    verdict: GateVerdict,
    status: LineageStatus,
    delta_sharpe: f64,
    /// F13: the candidate's metrics on the day + held-out windows, persisted so
    /// the historic-run detail can show per-candidate backtest results.
    child_day: MetricsSummary,
    child_untouched: MetricsSummary,
    /// Phase 2: per-regime evaluation rows. Empty when `regime_set` is empty
    /// (legacy / single-window path).
    regime_rows: Vec<RegimeResultRow>,
    /// Gate record scores — the oriented objective values for parent+child on
    /// both windows, plus the epsilon threshold and drawdown ratio.  `None` for
    /// regime-matrix paths where no single day/holdout pair is definitive.
    gate_scores: Option<GateScores>,
    /// WS-11b: the persisted eval `Run.id` for this candidate's PRIMARY
    /// day-window evaluation (the `paper_tester.run_with_run_id` call on the
    /// legacy / scenario-pool path). `None` on the regime-matrix path (which
    /// runs several evals and has no single definitive run) and for any
    /// `PaperTestRunner` that doesn't surface a run id (test stubs). Threaded
    /// onto `CycleProgressEvent::MutationGated` so the frontend can nest a
    /// navigable eval-run node under the experiment row.
    eval_run_id: Option<String>,
}

/// Numeric gate inputs captured at gate-verdict time so they can be persisted
/// to `autooptimizer_gate_records` by `process_parent_mutations`.
struct GateScores {
    pub parent_day_score: f64,
    pub child_day_score: f64,
    pub parent_holdout_score: f64,
    pub child_holdout_score: f64,
    pub gate_epsilon: f64,
    pub delta_day: f64,
    /// Holdout (untouched) gate threshold, separate from day `gate_epsilon`.
    #[allow(dead_code)] // persisted to DB in follow-up migration
    pub holdout_epsilon: f64,
    pub delta_holdout: f64,
    pub drawdown_ratio: Option<f64>,
    /// Edge vs a fixed-seed random baseline (informational, never gating).
    /// `None` when the baseline run was unavailable for this cycle.
    pub edge_over_random: Option<f64>,
    pub parent_edge: Option<f64>,
    pub edge_delta: Option<f64>,
}

/// Circuit-breaker tracking consecutive candidate eval failures.
///
/// Each `gate_and_classify` failure calls `record_failure()` which returns
/// `true` when the trip threshold is reached. A successful eval resets the
/// consecutive counter via `record_success()`. `max == 0` disables the breaker
/// (never trips). 2026-06-13 trader-failure resilience.
pub(crate) struct ConsecutiveErrors {
    count: u32,
    max: u32,
}

impl ConsecutiveErrors {
    pub(crate) fn new(max: u32) -> Self {
        Self { count: 0, max }
    }

    /// Increment the consecutive-failure counter.
    /// Returns `true` when the circuit trips (`count >= max` and `max > 0`).
    pub(crate) fn record_failure(&mut self) -> bool {
        self.count += 1;
        self.max > 0 && self.count >= self.max
    }

    /// Reset the consecutive-failure counter (a success breaks the streak).
    pub(crate) fn record_success(&mut self) {
        self.count = 0;
    }
}

#[cfg(test)]
mod consecutive_errors_tests {
    use super::ConsecutiveErrors;

    #[test]
    fn trips_at_max_consecutive() {
        let mut b = ConsecutiveErrors::new(3);
        assert!(!b.record_failure(), "1st failure must not trip");
        assert!(!b.record_failure(), "2nd failure must not trip");
        assert!(b.record_failure(), "3rd consecutive failure must trip");
    }

    #[test]
    fn success_resets_the_streak() {
        let mut b = ConsecutiveErrors::new(3);
        assert!(!b.record_failure());
        assert!(!b.record_failure()); // streak = 2
        b.record_success(); // reset
        assert!(!b.record_failure());
        assert!(!b.record_failure()); // streak = 2 again; 4 total failures, never 3 in a row
    }

    #[test]
    fn zero_max_disables_the_breaker() {
        let mut b = ConsecutiveErrors::new(0);
        for _ in 0..10 {
            assert!(!b.record_failure(), "max=0 must never trip");
        }
    }
}

/// Per-cycle memoization of the random-baseline objective score, keyed by
/// (day-scenario id, direction). The baseline depends only on the training
/// window + direction, so it is computed once and reused for every candidate.
type BaselineCache = tokio::sync::Mutex<std::collections::HashMap<(String, TradeDirection), f64>>;

/// Compute (memoized) the random-baseline objective score for `day_scenario`
/// under `direction`, using `structure_strategy` for risk sizing / filters.
/// Returns `f64::NAN` when the baseline run is unavailable; the caller maps NAN
/// to "no edge metrics" (the metric is informational and never blocks).
async fn random_baseline_score(
    paper_tester: &dyn PaperTestRunner,
    structure_strategy: &Strategy,
    day_scenario: &Scenario,
    direction: TradeDirection,
    objective: Objective,
    cache: &BaselineCache,
) -> f64 {
    let key = (day_scenario.id.clone(), direction);
    if let Some(v) = cache.lock().await.get(&key).copied() {
        return v;
    }
    let score = match paper_tester
        .run_random_baseline(structure_strategy, day_scenario, direction)
        .await
    {
        Ok(metrics) => objective.oriented_value(&metrics),
        Err(e) => {
            tracing::warn!(error = %e, "random baseline run failed; edge metrics omitted this cycle");
            f64::NAN
        }
    };
    cache.lock().await.insert(key, score);
    score
}

pub async fn run_cycle(
    pool: &SqlitePool,
    strategy_blob_store: &BlobStore,
    config: &AutoOptimizerConfig,
    cycle_config: &CycleConfig,
    parent_policy: &ParentPolicy,
    mutator: &Mutator,
    judge: &Judge,
    paper_tester: &dyn PaperTestRunner,
    progress: impl Fn(CycleProgressEvent) + Send + Sync,
    dspy_ctx: Option<&DspyContext>,
    // P2 (cortex-memory): when `Some`, the Judge recalls prior distilled
    // findings before judging and records new ones back to
    // `autooptimizer:judge`. `None` = today's behavior (default off).
    memory: Option<&crate::agent::memory_recorder::MemoryRecorder>,
    cycle_id_override: Option<String>,
    // F28: a cooperative cancel flag. When set, the cycle stops launching further
    // mutations/backtests (checked between candidates and parents) so an operator
    // can halt a long/expensive run. `None` = never cancelled.
    cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    // P4: a cooperative pause flag. When set, the cycle suspends at its next safe
    // checkpoint (between candidates) and polls every 1s until the flag is cleared
    // (resume) or the cancel flag is set. `None` = never paused.
    pause: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<CycleResult> {
    // [loosening-disabled 2026-06-22] use config value directly;
    // was: effective_min_improvement_for_cycle(pool, config, 0, cycle_config.sustained_no_pass_cycles)
    //         .await?.effective_min_improvement
    let cycle_id = cycle_id_override.unwrap_or_else(|| Ulid::new().to_string());
    let min_improvement = config.min_improvement;
    let holdout_min_improvement = config.holdout_min_improvement;
    let dsr_prefix: Option<String> = match dspy_ctx {
        Some(ctx) if config.dspy_enabled => query_dsr_prefix(&ctx.store, &ctx.namespace).await?,
        _ => None,
    };

    let lineage_store = LineageStore::new(pool.clone());
    let parents = if cycle_config.explicit_parent_hashes.is_empty() {
        select_parents(
            parent_policy,
            &lineage_store,
            cycle_config.num_parents,
            cycle_config.sabotage_seed,
        )
        .await?
    } else {
        let mut explicit = Vec::with_capacity(cycle_config.explicit_parent_hashes.len());
        for hash in &cycle_config.explicit_parent_hashes {
            let Some(node) = lineage_store.get(hash).await? else {
                bail!("explicit parent {} not found in lineage", hash.to_hex());
            };
            if node.status != LineageStatus::Active {
                bail!("explicit parent {} is not active", hash.to_hex());
            }
            explicit.push(node);
        }
        explicit
    };
    progress(CycleProgressEvent::CycleStarted {
        session_id: String::new(),
        cycle_id: cycle_id.clone(),
        parent_count: parents.len(),
    });

    let mut active_nodes: Vec<LineageNode> = Vec::new();
    let mut suspect_nodes: Vec<LineageNode> = Vec::new();
    let mut rejected_nodes: Vec<LineageNode> = Vec::new();
    let mut findings_by_node: HashMap<ContentHash, Vec<Finding>> = HashMap::new();
    let mut no_candidate_count: usize = 0;
    let mut errored_count: usize = 0;

    let is_cancelled = || {
        cancel
            .as_ref()
            .is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed))
    };

    // Memoizes the random-baseline objective score per (training window,
    // direction) for the whole cycle, so the extra backtest runs at most once.
    let baseline_cache: BaselineCache = BaselineCache::default();

    for parent_node in &parents {
        if is_cancelled() {
            break;
        }
        // P4: pause checkpoint — suspend here between parents if pause is set,
        // polling every 1s until the flag is cleared (resume) or the cycle is
        // cancelled. This is the outer per-parent boundary; the inner per-mutation
        // boundary in `process_parent_mutations` does the same check.
        while pause
            .as_ref()
            .is_some_and(|p| p.load(std::sync::atomic::Ordering::Relaxed))
        {
            if is_cancelled() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        if is_cancelled() {
            break;
        }
        let ph = parent_node.bundle_hash.to_hex();
        if let Some(parent_strategy) = cycle_config.parent_strategies.get(&ph) {
            progress(CycleProgressEvent::ParentSelected {
                session_id: String::new(),
                cycle_id: cycle_id.clone(),
                parent_hash: ph,
            });
            let (active, suspect, rejected, nc, ec) = process_parent_mutations(
                pool,
                strategy_blob_store,
                parent_node,
                parent_strategy,
                &cycle_id,
                min_improvement,
                holdout_min_improvement,
                cycle_config,
                config,
                mutator,
                judge,
                paper_tester,
                &baseline_cache,
                &progress,
                &mut findings_by_node,
                dsr_prefix.as_deref(),
                dspy_ctx,
                memory,
                cancel.as_ref(),
                pause.as_ref(),
            )
            .await?;
            active_nodes.extend(active);
            suspect_nodes.extend(suspect);
            rejected_nodes.extend(rejected);
            no_candidate_count += nc;
            errored_count += ec;
        }
    }

    // run-7: when this cycle produced NO gated candidate (every parent yielded
    // no_candidate, so nothing was backtested), running the sabotage/canary evals
    // is pure waste — there is no candidate whose gating to sanity-check. Skip it
    // and record a skipped result, mirroring the existing "no canary parent"
    // skipped representation (sabotage_variant "none" + a skipped message).
    progress(CycleProgressEvent::PhaseStarted {
        session_id: String::new(),
        cycle_id: cycle_id.clone(),
        parent_hash: None,
        phase: Phase::HonestyCheck,
        detail: "Running honesty check (sabotage canary)".to_string(),
    });
    let honesty_t0 = Instant::now();
    let honesty_check = if honesty_check_warranted(&active_nodes, &suspect_nodes, &rejected_nodes) {
        run_cycle_canary(
            &parents,
            cycle_config,
            config,
            mutator,
            paper_tester,
            min_improvement,
            holdout_min_improvement,
            &cycle_id,
            &progress,
        )
        .await?
    } else {
        let skipped = HonestyCheckResult {
            parent_hash: ContentHash::of_bytes(b""),
            gate_verdict: GateVerdict::Fail {
                reason: "no candidate produced".to_string(),
            },
            passed_check: true,
            sabotage_variant: "none".to_string(),
            message: "Honesty check skipped: no candidate was produced this cycle (nothing to verify)."
                .to_string(),
        };
        progress(CycleProgressEvent::HonestyCheckRun {
            session_id: String::new(),
            cycle_id: cycle_id.clone(),
            passed: skipped.passed_check,
            sabotage_variant: skipped.sabotage_variant.clone(),
            message: skipped.message.clone(),
        });
        skipped
    };
    progress(CycleProgressEvent::PhaseFinished {
        session_id: String::new(),
        cycle_id: cycle_id.clone(),
        parent_hash: None,
        phase: Phase::HonestyCheck,
        duration_ms: honesty_t0.elapsed().as_millis() as u64,
    });
    // F13: persist the honesty-check outcome so a historic cycle's detail can
    // report it (it was previously emitted only over SSE / the CLI summary).
    persist_honesty_check(pool, &cycle_id, &honesty_check).await;
    let diversity_score = diversity_decay_for_cycle(pool, &cycle_id).await.unwrap_or(0.0);
    Ok(CycleResult {
        cycle_id,
        active_nodes,
        suspect_nodes,
        rejected_nodes,
        honesty_check,
        diversity_score,
        findings_by_node,
        no_candidate_count,
        errored_count,
    })
}

/// The honesty check (sabotage canary) only adds value when the cycle actually
/// gated a candidate. When every parent yielded `no_candidate` — so nothing was
/// backtested — there is no gating to sanity-check, and running the canary evals
/// is pure waste (run-7 finding). Returns `true` iff at least one candidate was
/// gated (kept or rejected) this cycle.
fn honesty_check_warranted(
    active: &[LineageNode],
    suspect: &[LineageNode],
    rejected: &[LineageNode],
) -> bool {
    !active.is_empty() || !suspect.is_empty() || !rejected.is_empty()
}

async fn run_cycle_canary<F>(
    parents: &[LineageNode],
    cycle_config: &CycleConfig,
    config: &AutoOptimizerConfig,
    mutator: &Mutator,
    paper_tester: &dyn PaperTestRunner,
    min_improvement: f64,
    holdout_min_improvement: f64,
    cycle_id: &str,
    progress: &F,
) -> Result<HonestyCheckResult>
where
    F: Fn(CycleProgressEvent),
{
    let canary_parent = parents.iter().find(|n| {
        cycle_config
            .parent_strategies
            .contains_key(&n.bundle_hash.to_hex())
    });
    let Some(cn) = canary_parent else {
        return Ok(HonestyCheckResult {
            parent_hash: ContentHash::of_bytes(b""),
            gate_verdict: GateVerdict::Fail {
                reason: "no canary parent available".to_string(),
            },
            passed_check: true,
            sabotage_variant: "none".to_string(),
            message: "Honesty check skipped: no canary parent available this cycle.".to_string(),
        });
    };
    let s = &cycle_config.parent_strategies[&cn.bundle_hash.to_hex()];
    let mi = min_improvement;
    let hmi = holdout_min_improvement;
    let obj = cycle_config.objective;
    // R2: `run_honesty_check` itself degrades a canary eval/trader error to a
    // neutral failed-canary result (it never errors on canary eval), so this
    // `?` only ever propagates a genuine internal bug (e.g. strategy
    // serialization) — which cycle-level isolation (R3) then seals.
    let check = run_honesty_check(
        s,
        mutator,
        paper_tester,
        move |pd, cd, pu, cu| GateInput {
            parent_day_metrics: pd.clone(),
            child_day_metrics: cd.clone(),
            parent_untouched_metrics: pu.clone(),
            child_untouched_metrics: cu.clone(),
            min_improvement: mi,
            holdout_min_improvement: hmi,
            objective: obj,
        },
        &cycle_config.day_scenario,
        &cycle_config.baseline_scenario,
        config,
        cycle_config.sabotage_seed,
    )
    .await?;
    progress(CycleProgressEvent::HonestyCheckRun {
        session_id: String::new(),
        cycle_id: cycle_id.to_string(),
        passed: check.passed_check,
        sabotage_variant: check.sabotage_variant.clone(),
        message: check.message.clone(),
    });
    Ok(check)
}

/// F32: derive a deterministic exploration seed for the experiment writer from
/// the (unique-per-cycle) `cycle_id` mixed with the mutation index. FNV-1a — no
/// external deps, stable within a build, and varies every cycle so successive
/// cycles on the same parent propose diverse candidates instead of one fixed
/// tweak. A test asserts distinct seeds yield distinct candidates.
pub fn exploration_seed_for(cycle_id: &str, mutation_idx: usize) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in cycle_id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= mutation_idx as u64;
    h.wrapping_mul(0x0000_0100_0000_01b3)
}

/// B19: round-robin selection of the `(day_scenario, baseline_scenario)` pair a
/// given candidate is evaluated on.
///
/// - When `pool` is empty, returns the `fallback` pair (the single
///   `day_window`/`baseline_untouched_window` pair) for EVERY candidate — i.e.
///   the legacy behavior, unchanged.
/// - When `pool` is non-empty, returns `pool[mutation_idx % pool.len()]`, so the
///   pairs cycle deterministically across candidates.
///
/// Pure and deterministic so the round-robin can be unit-tested without running
/// any backtests. Returned references borrow from `pool`/`fallback`; both the
/// child evaluation AND the parent baseline it is compared against MUST use the
/// returned pair to keep the gate comparison valid (B19 comparability rule).
pub fn select_scenario_pair<'a>(
    pool: &'a [(Scenario, Scenario)],
    fallback: (&'a Scenario, &'a Scenario),
    mutation_idx: usize,
) -> (&'a Scenario, &'a Scenario) {
    if pool.is_empty() {
        return fallback;
    }
    let (day, baseline) = &pool[mutation_idx % pool.len()];
    (day, baseline)
}

async fn process_parent_mutations<F>(
    pool: &SqlitePool,
    strategy_blob_store: &BlobStore,
    parent_node: &LineageNode,
    parent_strategy: &Strategy,
    cycle_id: &str,
    min_improvement: f64,
    holdout_min_improvement: f64,
    cycle_config: &CycleConfig,
    config: &AutoOptimizerConfig,
    mutator: &Mutator,
    judge: &Judge,
    paper_tester: &dyn PaperTestRunner,
    baseline_cache: &BaselineCache,
    progress: &F,
    _findings_by_node: &mut HashMap<ContentHash, Vec<Finding>>,
    dsr_prefix: Option<&str>,
    dspy_ctx: Option<&DspyContext>,
    memory: Option<&crate::agent::memory_recorder::MemoryRecorder>,
    cancel: Option<&std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pause: Option<&std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(Vec<LineageNode>, Vec<LineageNode>, Vec<LineageNode>, usize, usize)>
where
    F: Fn(CycleProgressEvent),
{
    assert!(
        cycle_config.mutations_per_parent <= 64,
        "mutations_per_parent exceeds bound"
    );
    let mut active: Vec<LineageNode> = Vec::new();
    let mut suspect: Vec<LineageNode> = Vec::new();
    let mut rejected: Vec<LineageNode> = Vec::new();
    let mut no_candidate_count: usize = 0;
    let mut errored_count: usize = 0;
    let mut breaker = ConsecutiveErrors::new(cycle_config.max_consecutive_errors);

    // B19: when the scenario_pool is active, the parent must be re-evaluated on
    // EACH sampled pair (a child is compared only against its parent on the SAME
    // pair — comparability rule). Those per-pair parent metrics are cached lazily
    // below (`parent_pool_metrics`). The eager single-pair parent backtest is
    // therefore only meaningful for the legacy single-pair path; skip it when a
    // pool is configured to avoid a wasted backtest on a pair no candidate may
    // even use.
    let scenario_pool_active = cycle_config.regime_set.is_empty() && !cycle_config.scenario_pool.is_empty();

    // Fix 6: Only backtest the parent on the legacy day/baseline scenarios when
    // regime_set is empty. When regime_set is non-empty, the legacy day+baseline
    // metrics are never used for classification — only per-regime backtests drive
    // the gate. Skipping these runs avoids wasted backtests. B19: also skip when
    // a scenario_pool is active (per-pair parent metrics are computed lazily).
    let (parent_day, parent_untouched) = if cycle_config.regime_set.is_empty() && !scenario_pool_active {
        // U5: bracket the parent baseline backtests with phase boundaries so the
        // cycle output stream isn't silent for the 10–20 minutes these two
        // (full-window) backtests take. Before this, the only event between
        // `ParentSelected` and the first candidate's `PhaseStarted` was nothing —
        // operators read the gap as a hang and cancelled the cycle. The
        // underlying executor also emits `EvalHeartbeat` on its ProgressTx (wired
        // via `CachedBacktestPaperTester::with_progress_bus`); a CLI/dashboard
        // bridge drains that and re-emits `CycleProgressEvent::EvalProgress`.
        progress(CycleProgressEvent::PhaseStarted {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: Some(parent_node.bundle_hash.to_hex()),
            phase: Phase::EvalDayWindow,
            detail: "Evaluating parent baseline on the day window".to_string(),
        });
        let day_t0 = Instant::now();
        let pd = paper_tester
            .run(parent_strategy, &cycle_config.day_scenario)
            .await?;
        progress(CycleProgressEvent::PhaseFinished {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: Some(parent_node.bundle_hash.to_hex()),
            phase: Phase::EvalDayWindow,
            duration_ms: day_t0.elapsed().as_millis() as u64,
        });
        progress(CycleProgressEvent::PhaseStarted {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: Some(parent_node.bundle_hash.to_hex()),
            phase: Phase::EvalUntouchedWindow,
            detail: "Evaluating parent baseline on the untouched window".to_string(),
        });
        let unt_t0 = Instant::now();
        let pu = paper_tester
            .run(parent_strategy, &cycle_config.baseline_scenario)
            .await?;
        progress(CycleProgressEvent::PhaseFinished {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: Some(parent_node.bundle_hash.to_hex()),
            phase: Phase::EvalUntouchedWindow,
            duration_ms: unt_t0.elapsed().as_millis() as u64,
        });
        (pd, pu)
    } else {
        (MetricsSummary::default(), MetricsSummary::default())
    };

    // B19: per-pair parent metrics cache, keyed by BOTH sampled scenario ids.
    // The baseline id is part of the key because two pool entries may reuse the
    // same training/day window with different holdout windows. In that shape,
    // keying only by day id would incorrectly reuse the first holdout metrics
    // and break the parent/child comparability rule.
    let mut parent_pool_metrics: HashMap<(String, String), (MetricsSummary, MetricsSummary)> = HashMap::new();

    // Fix 2 + 3: validate regime set (duplicate labels, day/baseline overlap)
    // before entering the mutation loop. Returns immediately on the first
    // violation so invalid configs are caught before any backtest cost is paid.
    if !cycle_config.regime_set.is_empty() {
        validate_regime_set(&cycle_config.regime_set)?;
    }

    // Phase 2: pre-compute parent metrics once per regime window so every child
    // mutation in this loop can reuse them. Empty when regime_set is empty.
    let mut parent_regime_metrics: HashMap<String, (MetricsSummary, MetricsSummary)> = HashMap::new();
    for rw in &cycle_config.regime_set {
        let (regime_day_scen, regime_baseline_scen) = build_regime_scenario_pair(cycle_config, rw)?;
        let pd = paper_tester.run(parent_strategy, &regime_day_scen).await?;
        let pu = paper_tester.run(parent_strategy, &regime_baseline_scen).await?;
        parent_regime_metrics.insert(rw.label.clone(), (pd, pu));
    }

    // P3: recall prior optimizer outcomes on similar strategies (once per
    // parent) to advise the experiment writer. Best-effort and eval-temporal-
    // safe — forward the scenario start so Patterns trained inside the window
    // can't leak; any recall error / missing embedder degrades to `None`
    // (plain prompt) and never fails the cycle. This is advisory only; the F32
    // exploration seed + hard avoid-set still govern exact repeats.
    let mutation_memory_context: Option<String> = match memory {
        None => None,
        Some(mem) => {
            let query = crate::autooptimizer::program_view::to_markdown(parent_strategy);
            match mem
                .recall_in_namespace(
                    crate::autooptimizer::mutator::MUTATIONS_NS,
                    &query,
                    5,
                    Some(cycle_config.day_scenario.time_window.start),
                )
                .await
            {
                Ok(crate::agent::memory_recorder::RecallResult::Hits { matches, .. })
                    if !matches.is_empty() =>
                {
                    Some(crate::agent::memory_recorder::render_recalled_patterns(&matches))
                }
                Ok(_) => None,
                Err(e) => {
                    tracing::warn!("mutator memory recall failed (best-effort, ignoring): {e}");
                    None
                }
            }
        }
    };
    // F32: every candidate this parent has ALREADY produced (across all prior
    // cycles), so the mutator can be steered away from re-deriving them and any
    // duplicate is dropped before it spends a backtest. Best-effort: a lineage
    // read failure just yields an empty set (the in-mutator guard still holds for
    // candidates produced within this cycle). Accumulates within the cycle too.
    let lineage = LineageStore::new(pool.clone());
    let mut avoid: HashSet<ContentHash> = lineage
        .children_of(&parent_node.bundle_hash)
        .await
        .map(|kids| kids.into_iter().map(|n| n.bundle_hash).collect())
        .unwrap_or_default();

    // Resolve each agent's real system_prompt so the experiment writer sees
    // the actual trading logic rather than just an agent_id reference. Only
    // agents whose prompt_override is None need resolution — if an override is
    // already set, it's visible in the AgentRef JSON block. Best-effort: a DB
    // miss (agent not found) just leaves that agent un-annotated, which is
    // no worse than the previous behaviour.
    let resolved_agent_prompts: HashMap<String, String> = {
        let agent_store = crate::agents::AgentStore::new(pool.clone());
        let mut map = HashMap::new();
        for agent_ref in &parent_strategy.agents {
            if agent_ref.prompt_override.is_none() {
                match agent_store.get(&agent_ref.agent_id).await {
                    Ok(Some(agent)) => {
                        if let Some(slot) = agent.slots.first() {
                            if !slot.system_prompt.is_empty() {
                                map.insert(agent_ref.agent_id.clone(), slot.system_prompt.clone());
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(
                            agent_id = %agent_ref.agent_id,
                            error = %e,
                            "failed to resolve agent prompt for mutator context (best-effort, ignoring)"
                        );
                    }
                }
            }
        }
        map
    };

    for mutation_idx in 0..cycle_config.mutations_per_parent {
        // F28: stop launching further candidates once the operator cancels.
        if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
            break;
        }
        // P4: pause checkpoint — suspend here between mutations if pause flag is
        // set; poll every 1s until cleared (resume) or cancelled.
        while pause.is_some_and(|p| p.load(std::sync::atomic::Ordering::Relaxed)) {
            if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
            break;
        }
        // F32: a per-cycle, per-mutation exploration seed so the experiment writer
        // proposes DIVERSE candidates across cycles (it was deterministic — same
        // parent always yielded the identical candidate, so the optimizer never
        // explored). The cycle_id is unique per cycle (ULID), so hashing it varies
        // the seed every run; mixing the mutation index varies it within a cycle.
        let exploration_seed = exploration_seed_for(cycle_id, mutation_idx);
        // When tournament_enabled, run the 3-candidate Borda-count tournament
        // instead of a direct propose call. Incumbent win means no candidate
        // beat the parent this iteration.
        //
        // F14/F15 (2026-06-04): every "no usable candidate" branch now emits a
        // `NoCandidate` event instead of a silent `continue`, so a cycle that
        // produced nothing is distinguishable in the CLI summary and panel from
        // one that gated a real experiment.
        let ph_str = parent_node.bundle_hash.to_hex();
        progress(CycleProgressEvent::PhaseStarted {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: Some(ph_str.clone()),
            phase: Phase::WriterProposing,
            detail: "Experiment writer proposing candidate".to_string(),
        });
        let writer_t0 = Instant::now();
        let mut tournament_votes: Option<
            Vec<(
                crate::autooptimizer::tournament::JudgePersona,
                crate::autooptimizer::tournament::BordaVote,
            )>,
        > = None;
        let diff_result: Option<crate::autooptimizer::mutator::MutationDiff> = if config.tournament_enabled {
            use crate::autooptimizer::tournament::TournamentRunner;
            let runner = TournamentRunner::from_mutator(mutator);
            match runner
                .run_tournament(parent_strategy, config, Some(&resolved_agent_prompts))
                .await
            {
                Ok(r) if r.incumbent_wins => {
                    no_candidate_count += 1;
                    progress(CycleProgressEvent::PhaseFinished {
                        session_id: String::new(),
                        cycle_id: cycle_id.to_string(),
                        parent_hash: Some(ph_str.clone()),
                        phase: Phase::WriterProposing,
                        duration_ms: writer_t0.elapsed().as_millis() as u64,
                    });
                    progress(CycleProgressEvent::NoCandidate {
                        session_id: String::new(),
                        cycle_id: cycle_id.to_string(),
                        parent_hash: parent_node.bundle_hash.to_hex(),
                        reason: "tournament incumbent retained (no challenger improved on the parent)"
                            .to_string(),
                    });
                    continue;
                }
                Ok(r) => {
                    tournament_votes = Some(r.per_persona_votes);
                    Some(r.winner_diff)
                }
                Err(e) => {
                    no_candidate_count += 1;
                    progress(CycleProgressEvent::PhaseFinished {
                        session_id: String::new(),
                        cycle_id: cycle_id.to_string(),
                        parent_hash: Some(ph_str.clone()),
                        phase: Phase::WriterProposing,
                        duration_ms: writer_t0.elapsed().as_millis() as u64,
                    });
                    progress(CycleProgressEvent::NoCandidate {
                        session_id: String::new(),
                        cycle_id: cycle_id.to_string(),
                        parent_hash: parent_node.bundle_hash.to_hex(),
                        reason: format!("tournament failed: {e}"),
                    });
                    continue;
                }
            }
        } else {
            match mutator
                .propose(
                    parent_strategy,
                    config,
                    dsr_prefix,
                    exploration_seed,
                    mutation_idx,
                    mutation_memory_context.as_deref(),
                    &avoid,
                    Some(&resolved_agent_prompts),
                )
                .await
            {
                Ok(d) => Some(d),
                Err(e) => {
                    // The mutator exhausted its retries without a distinct,
                    // valid diff (e.g. every attempt was an identity/no-op —
                    // F14). Surface it rather than exiting the iteration silently.
                    no_candidate_count += 1;
                    progress(CycleProgressEvent::PhaseFinished {
                        session_id: String::new(),
                        cycle_id: cycle_id.to_string(),
                        parent_hash: Some(ph_str.clone()),
                        phase: Phase::WriterProposing,
                        duration_ms: writer_t0.elapsed().as_millis() as u64,
                    });
                    progress(CycleProgressEvent::NoCandidate {
                        session_id: String::new(),
                        cycle_id: cycle_id.to_string(),
                        parent_hash: parent_node.bundle_hash.to_hex(),
                        reason: format!("experiment writer produced no usable candidate: {e}"),
                    });
                    continue;
                }
            }
        };
        let diff = diff_result.expect("diff_result is None only on continue paths above");
        // Phase 2 dimension gate: simplicity — reject parameter explosions before
        // spending backtest tokens. The numeric gate still runs afterward for
        // candidates that pass this dimension.
        {
            let simplicity_verdict = crate::autooptimizer::gate::check_dimension_simplicity(&diff);
            if let GateVerdict::Fail { reason } = &simplicity_verdict {
                progress(CycleProgressEvent::NoCandidate {
                    session_id: String::new(),
                    cycle_id: cycle_id.to_string(),
                    parent_hash: parent_node.bundle_hash.to_hex(),
                    reason: format!("dimension gate (simplicity): {reason}"),
                });
                no_candidate_count += 1;
                continue;
            }
        }
        progress(CycleProgressEvent::PhaseFinished {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: Some(ph_str.clone()),
            phase: Phase::WriterProposing,
            duration_ms: writer_t0.elapsed().as_millis() as u64,
        });
        // F12 defensive guard: never gate or persist a child byte-identical to
        // its parent. The mutator already rejects identity diffs, but the
        // tournament path does not, and a no-op child's content hash equals the
        // parent's — inserting it would overwrite (corrupt) the parent node and
        // create a self-parent cycle in the lineage graph.
        let candidate = diff.apply_to(parent_strategy);
        let candidate_hash = serde_json::to_value(&candidate)
            .map(|v| ContentHash::of_json(&v))
            .ok();
        let is_identity = candidate_hash.as_ref() == Some(&parent_node.bundle_hash);
        if is_identity {
            tracing::debug!(
                cycle_id,
                parent_hash = %parent_node.bundle_hash.to_hex(),
                "skipping identity (no-op) mutation diff — guaranteed 0.0 delta, no backtest spent",
            );
            no_candidate_count += 1;
            progress(CycleProgressEvent::NoCandidate {
                session_id: String::new(),
                cycle_id: cycle_id.to_string(),
                parent_hash: parent_node.bundle_hash.to_hex(),
                reason: "experiment writer produced a no-op (identity) diff".to_string(),
            });
            continue;
        }
        // F32 backstop: drop a candidate this parent has already produced (in a
        // prior cycle OR earlier this cycle) before it spends four backtests on a
        // known result. The mutator's `already_tried` retry normally prevents this,
        // but this also covers the tournament path and any model that ignores the
        // retry hint — the hard guarantee that repeat cycles can't re-evaluate the
        // same candidate forever.
        if let Some(h) = candidate_hash.as_ref() {
            if avoid.contains(h) {
                tracing::debug!(
                    cycle_id,
                    parent_hash = %parent_node.bundle_hash.to_hex(),
                    child_hash = %h.to_hex(),
                    "skipping duplicate candidate already evaluated on this parent — no backtest spent",
                );
                no_candidate_count += 1;
                progress(CycleProgressEvent::NoCandidate {
                    session_id: String::new(),
                    cycle_id: cycle_id.to_string(),
                    parent_hash: parent_node.bundle_hash.to_hex(),
                    reason: "experiment writer re-derived a candidate already evaluated on this parent"
                        .to_string(),
                });
                continue;
            }
        }
        progress(CycleProgressEvent::MutationProposed {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: parent_node.bundle_hash.to_hex(),
            child_hash: candidate_hash.as_ref().map(|h| h.to_hex()).unwrap_or_default(),
            mutator_model: mutator.model.clone(),
        });
        progress(CycleProgressEvent::PhaseStarted {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: Some(ph_str.clone()),
            phase: Phase::GateEvaluating,
            detail: "Running backtests and numeric gate".to_string(),
        });
        // B19: round-robin select THIS candidate's (day, baseline) pair from the
        // pool. On the legacy / regime paths the pool is empty, so the fallback
        // (the single cycle day/baseline pair) is returned for every candidate —
        // behavior identical to before. The selected pair is then used for BOTH
        // the child eval AND the parent baseline it is compared against, so the
        // gate comparison stays valid (comparability rule).
        let (sampled_day, sampled_baseline) = select_scenario_pair(
            &cycle_config.scenario_pool,
            (&cycle_config.day_scenario, &cycle_config.baseline_scenario),
            mutation_idx,
        );
        // B19 comparability: parent metrics MUST come from the SAME sampled pair.
        // For the pool path, compute (and cache by sampled pair) the parent's
        // day+baseline metrics on this pair; for the legacy/regime paths reuse
        // the pre-computed `parent_day`/`parent_untouched` exactly as before.
        let (gate_parent_day, gate_parent_untouched) = if scenario_pool_active {
            let pair_key = (sampled_day.id.clone(), sampled_baseline.id.clone());
            if !parent_pool_metrics.contains_key(&pair_key) {
                let pd = paper_tester.run(parent_strategy, sampled_day).await?;
                let pu = paper_tester.run(parent_strategy, sampled_baseline).await?;
                parent_pool_metrics.insert(pair_key.clone(), (pd, pu));
            }
            let (pd, pu) = parent_pool_metrics
                .get(&pair_key)
                .expect("parent pool metrics just inserted");
            (pd.clone(), pu.clone())
        } else {
            (parent_day.clone(), parent_untouched.clone())
        };
        if scenario_pool_active {
            // B19 observability: the proposal currently gives operators no way to
            // tell which regime a candidate was scored on. Emit the sampled pair's
            // label (display_name carries the window) so the round-robin is visible
            // in cycle logs / SSE-adjacent tracing.
            tracing::info!(
                cycle_id,
                parent_hash = %ph_str,
                mutation_idx,
                scenario_label = %sampled_day.display_name,
                scenario_day = %sampled_day.description,
                "B19 round-robin: candidate evaluated on sampled scenario pair"
            );
        }
        let gate_t0 = Instant::now();
        let gate_result = gate_and_classify(
            parent_strategy,
            diff,
            cycle_config,
            paper_tester,
            config.baseline_direction,
            baseline_cache,
            &gate_parent_day,
            &gate_parent_untouched,
            min_improvement,
            holdout_min_improvement,
            &parent_regime_metrics,
            sampled_day,
            sampled_baseline,
            progress,
            cycle_id,
            &ph_str,
        )
        .await;
        let outcome = match gate_result {
            Ok(o) => {
                breaker.record_success();
                progress(CycleProgressEvent::PhaseFinished {
                    session_id: String::new(),
                    cycle_id: cycle_id.to_string(),
                    parent_hash: Some(ph_str.clone()),
                    phase: Phase::GateEvaluating,
                    duration_ms: gate_t0.elapsed().as_millis() as u64,
                });
                o
            }
            Err(e) => {
                errored_count += 1;
                let tripped = breaker.record_failure();
                progress(CycleProgressEvent::PhaseFinished {
                    session_id: String::new(),
                    cycle_id: cycle_id.to_string(),
                    parent_hash: Some(ph_str.clone()),
                    phase: Phase::GateEvaluating,
                    duration_ms: gate_t0.elapsed().as_millis() as u64,
                });
                progress(CycleProgressEvent::CandidateError {
                    session_id: String::new(),
                    cycle_id: cycle_id.to_string(),
                    parent_hash: ph_str.clone(),
                    reason: format!("candidate eval failed: {e}"),
                });
                if tripped {
                    return Err(anyhow::anyhow!(
                        "optimizer halted: {} consecutive candidate eval failures \
                         (--max-consecutive-errors={}); last: {e}",
                        cycle_config.max_consecutive_errors,
                        cycle_config.max_consecutive_errors,
                    ));
                }
                continue;
            }
        };
        // Fix 1+2: build_and_insert_node now atomically writes node + regime rows
        // and returns the *resolved* status (which may be Active when the
        // collision-guard preserves an existing active node).  Emit the SSE event
        // and route the result bucket from the resolved status, not outcome.status.
        let (node, resolved_status) =
            build_and_insert_node(pool, strategy_blob_store, &outcome, parent_node, cycle_id).await?;
        let outcome_str = match &resolved_status {
            LineageStatus::Active => "kept",
            LineageStatus::Quarantined => "suspect",
            LineageStatus::Rejected => "dropped",
        };
        progress(CycleProgressEvent::MutationGated {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            child_hash: outcome.child_hash.to_hex(),
            passed: matches!(outcome.verdict, GateVerdict::Pass),
            outcome: outcome_str.to_string(),
            delta_day: outcome.gate_scores.as_ref().map(|gs| gs.delta_day),
            // WS-11b: the candidate's primary day-window eval run id, so the
            // dashboard can nest a navigable eval-run node under the experiment.
            eval_run_id: outcome.eval_run_id.clone(),
        });
        // P2-W2: persist gate record to autooptimizer_gate_records. Best-effort —
        // a DB error must never abort the cycle.
        if let Some(gs) = &outcome.gate_scores {
            let verdict_str = outcome.verdict.as_str();
            let reason_str = match &outcome.verdict {
                GateVerdict::Fail { reason } => Some(reason.as_str()),
                GateVerdict::Pass => None,
            };
            if let Err(e) = persist_gate_record(
                pool,
                GateRecord {
                    bundle_hash: &outcome.child_hash.to_hex(),
                    parent_day_score: Some(gs.parent_day_score),
                    child_day_score: Some(gs.child_day_score),
                    parent_holdout_score: Some(gs.parent_holdout_score),
                    child_holdout_score: Some(gs.child_holdout_score),
                    gate_epsilon: Some(gs.gate_epsilon),
                    delta_day: Some(gs.delta_day),
                    delta_holdout: Some(gs.delta_holdout),
                    drawdown_ratio: gs.drawdown_ratio,
                    verdict: &verdict_str,
                    reason: reason_str,
                    rationale: Some(outcome.diff.rationale.as_str()),
                    edge_over_random: gs.edge_over_random,
                    parent_edge: gs.parent_edge,
                    edge_delta: gs.edge_delta,
                },
            )
            .await
            {
                tracing::warn!(
                    cycle_id,
                    child_hash = %outcome.child_hash.to_hex(),
                    "failed to persist gate record (best-effort, ignoring): {e}"
                );
            }
        }
        // P3 write-back: record EVERY gated candidate's outcome (both Active and
        // Rejected) as an Observation in the mutations namespace, so a later
        // distillation pass can promote recurring lessons (e.g. "raising leverage
        // past Nx degraded holdout") into Patterns the experiment writer recalls
        // across runs. Best-effort and eval-temporal-safe; never fail the cycle.
        if let Some(mem) = memory {
            let status_label = match resolved_status {
                LineageStatus::Active => "active",
                LineageStatus::Quarantined => "suspect",
                LineageStatus::Rejected => "rejected",
            };
            let obs = crate::autooptimizer::mutator::describe_mutation_outcome(
                &outcome.diff,
                outcome.delta_sharpe,
                status_label,
            );
            if let Err(e) = mem
                .record_observation_in_namespace(
                    crate::autooptimizer::mutator::MUTATIONS_NS,
                    &obs,
                    cycle_id.to_string(),
                    "autooptimizer".to_string(),
                    0,
                    cycle_config.day_scenario.time_window.start,
                    cycle_config.day_scenario.time_window.end,
                )
                .await
            {
                tracing::warn!("mutator outcome write-back failed (best-effort, ignoring): {e}");
            }
        }
        // F32: remember this candidate so later mutations this cycle (and the
        // mutator's own retries) won't re-derive it.
        avoid.insert(outcome.child_hash);
        record_proposal(
            pool,
            &outcome.child_hash,
            &mutator.provider,
            &mutator.model,
            &cycle_config.prompt_version,
        )
        .await?;
        match resolved_status {
            LineageStatus::Active => {
                record_outcome(pool, &outcome.child_hash, outcome.delta_sharpe).await?;
                // P2: pass the recorder + eval scenario start so judge recall is
                // temporally safe (Patterns trained inside the scenario can't leak).
                progress(CycleProgressEvent::PhaseStarted {
                    session_id: String::new(),
                    cycle_id: cycle_id.to_string(),
                    parent_hash: Some(ph_str.clone()),
                    phase: Phase::ReviewerRunning,
                    detail: "Reviewer evaluating active candidate".to_string(),
                });
                let reviewer_t0 = Instant::now();
                let mut findings = run_judge(
                    judge,
                    parent_strategy,
                    &outcome.child,
                    &outcome.diff,
                    "",
                    memory,
                    Some(cycle_config.day_scenario.time_window.start),
                )
                .await?;
                progress(CycleProgressEvent::PhaseFinished {
                    session_id: String::new(),
                    cycle_id: cycle_id.to_string(),
                    parent_hash: Some(ph_str.clone()),
                    phase: Phase::ReviewerRunning,
                    duration_ms: reviewer_t0.elapsed().as_millis() as u64,
                });
                // Phase 5a: emit JUDGE_MISMATCH findings when persona rankings
                // disagree with the numeric gate verdict.
                if let Some(votes) = tournament_votes.take() {
                    let _gate_active = true; // LineageStatus::Active block — only active candidates get here
                    for (persona, vote) in &votes {
                        let label = persona.label();
                        let judge_top = vote.ranking[0];
                        let persona_preferred_winner = judge_top == 0;
                        if !persona_preferred_winner {
                            findings.push(crate::autooptimizer::judge::Finding {
                                code: "JUDGE_MISMATCH".to_string(),
                                severity: crate::autooptimizer::judge::FindingSeverity::Warn,
                                summary: format!(
                                    "{label} persona ranked candidate index {judge_top} as #1 \
                                     but the numeric gate kept the winner \
                                     (Δsharpe={:.4})",
                                    outcome.delta_sharpe,
                                ),
                                detail: Some(format!(
                                    "persona={label} judge_top={judge_top} winner=0 \
                                     delta_sharpe={:.4} min_improvement={:.4}",
                                    outcome.delta_sharpe, min_improvement,
                                )),
                            });
                        }
                    }
                }
                for f in &findings {
                    progress(CycleProgressEvent::JudgeFinding {
                        session_id: String::new(),
                        cycle_id: cycle_id.to_string(),
                        child_hash: outcome.child_hash.to_hex(),
                        severity: format!("{:?}", f.severity),
                        code: f.code.clone(),
                    });
                    // P2-W2: persist each finding to autooptimizer_findings.
                    let judge_model = format!("{}/{}", judge.provider, judge.model);
                    if let Err(e) =
                        persist_finding(pool, &outcome.child_hash.to_hex(), f, Some(judge_model.as_str()))
                            .await
                    {
                        tracing::warn!(
                            cycle_id,
                            child_hash = %outcome.child_hash.to_hex(),
                            "failed to persist judge finding (best-effort, ignoring): {e}"
                        );
                    }
                }
                // P2 write-back: record each real finding as an Observation in the
                // judge namespace so a later distillation pass can promote recurring
                // ones to Patterns. Best-effort — never fail the cycle on a memory
                // error; skip the synthetic parse-error finding.
                if let Some(mem) = memory {
                    for f in &findings {
                        if f.code == "parse_error" {
                            continue;
                        }
                        if let Err(e) = mem
                            .record_observation_in_namespace(
                                crate::autooptimizer::judge::JUDGE_MEMORY_NS,
                                &format!("[{}] {}", f.code, f.summary),
                                cycle_id.to_string(),
                                "autooptimizer".to_string(),
                                0,
                                cycle_config.day_scenario.time_window.start,
                                cycle_config.day_scenario.time_window.end,
                            )
                            .await
                        {
                            tracing::warn!("judge finding write-back failed (best-effort, ignoring): {e}");
                        }
                    }
                }
                if let Some(pattern_id) = handle_cycle_dspy(config, dspy_ctx, &findings, cycle_id).await? {
                    // DSPy flywheel compiled findings into a prompt pattern this cycle.
                    progress(CycleProgressEvent::FlywheelCompiled {
                        session_id: String::new(),
                        cycle_id: cycle_id.to_string(),
                        optimization_run_id: cycle_id.to_string(),
                        pattern_id,
                    });
                }
                // Phase 7: record findings in the anti-pattern registry.
                for f in &findings {
                    if f.code == "parse_error" {
                        continue;
                    }
                    if let Err(e) =
                        crate::autooptimizer::anti_pattern::record_finding(pool, &f.code, &f.summary).await
                    {
                        tracing::warn!("anti-pattern recording failed (best-effort): {e}");
                    }
                }
                active.push(node);
            }
            LineageStatus::Quarantined => {
                suspect.push(node);
            }
            LineageStatus::Rejected => {
                rejected.push(node);
            }
        }
    }
    Ok((active, suspect, rejected, no_candidate_count, errored_count))
}

async fn gate_and_classify<F>(
    parent_strategy: &Strategy,
    diff: MutationDiff,
    cycle_config: &CycleConfig,
    paper_tester: &dyn PaperTestRunner,
    baseline_direction: TradeDirection,
    baseline_cache: &BaselineCache,
    parent_day: &MetricsSummary,
    parent_untouched: &MetricsSummary,
    min_improvement: f64,
    holdout_min_improvement: f64,
    // Per-regime parent metrics (label → (day, untouched)), pre-computed by
    // `process_parent_mutations` so each parent is evaluated only once per
    // regime window across all its mutations.
    parent_regime_metrics: &HashMap<String, (MetricsSummary, MetricsSummary)>,
    // B19: the (day, baseline) scenario pair THIS candidate is evaluated on,
    // selected round-robin by the caller. On the legacy / regime paths this is
    // the single cycle day/baseline pair (`cycle_config.day_scenario` /
    // `baseline_scenario`); on the scenario_pool path it is the sampled pair.
    // The `parent_day`/`parent_untouched` passed above were computed on this SAME
    // pair (comparability), so the legacy gate path uses these scenarios for the
    // child eval, inversion check, and random-baseline edge — never the raw
    // `cycle_config` fields — to stay consistent with the parent metrics.
    sampled_day: &Scenario,
    sampled_baseline: &Scenario,
    // Progress callback + context for emitting inner phase events.
    progress: &F,
    cycle_id: &str,
    parent_hash_str: &str,
) -> Result<MutationOutcome>
where
    F: Fn(CycleProgressEvent),
{
    // R4: normalize the diff's stale `before` baselines to the parent's live
    // values BEFORE the candidate is gated and stored. Beyond the inversion
    // honesty-check, the stored diff feeds `describe_mutation_outcome` → the
    // optimizer-memory write-back; an un-normalized (model-hallucinated)
    // `before` would persist a fictitious baseline later recalled into the
    // experiment-writer prompt. `after` is never touched, so the forward child
    // and the lineage hash are unaffected.
    let mut diff = diff;
    crate::autooptimizer::inversion::normalize_prose_baseline(&mut diff, parent_strategy);
    crate::autooptimizer::inversion::normalize_filter_baseline(&mut diff, parent_strategy);
    crate::autooptimizer::inversion::normalize_param_baseline(&mut diff, parent_strategy);

    let child = diff.apply_to(parent_strategy);
    let child_hash = ContentHash::of_json(&serde_json::to_value(&child)?);

    // ── Phase 2: regime-matrix path ──────────────────────────────────────────
    if !cycle_config.regime_set.is_empty() {
        // Build (day, baseline) scenario pairs for every regime window.
        let mut regime_inputs: Vec<RegimeEvalInput> = Vec::with_capacity(cycle_config.regime_set.len());
        for rw in &cycle_config.regime_set {
            let (regime_day_scen, regime_baseline_scen) = build_regime_scenario_pair(cycle_config, rw)?;
            // EvalDayWindow phase for each regime.
            progress(CycleProgressEvent::PhaseStarted {
                session_id: String::new(),
                cycle_id: cycle_id.to_string(),
                parent_hash: Some(parent_hash_str.to_string()),
                phase: Phase::EvalDayWindow,
                detail: format!("Day-window backtest for regime '{}'", rw.label),
            });
            let t0 = Instant::now();
            let child_day_r = paper_tester.run(&child, &regime_day_scen).await?;
            progress(CycleProgressEvent::PhaseFinished {
                session_id: String::new(),
                cycle_id: cycle_id.to_string(),
                parent_hash: Some(parent_hash_str.to_string()),
                phase: Phase::EvalDayWindow,
                duration_ms: t0.elapsed().as_millis() as u64,
            });
            // EvalUntouchedWindow phase for each regime.
            progress(CycleProgressEvent::PhaseStarted {
                session_id: String::new(),
                cycle_id: cycle_id.to_string(),
                parent_hash: Some(parent_hash_str.to_string()),
                phase: Phase::EvalUntouchedWindow,
                detail: format!("Untouched-window backtest for regime '{}'", rw.label),
            });
            let t0 = Instant::now();
            let child_untouched_r = paper_tester.run(&child, &regime_baseline_scen).await?;
            progress(CycleProgressEvent::PhaseFinished {
                session_id: String::new(),
                cycle_id: cycle_id.to_string(),
                parent_hash: Some(parent_hash_str.to_string()),
                phase: Phase::EvalUntouchedWindow,
                duration_ms: t0.elapsed().as_millis() as u64,
            });
            let (parent_day_r, parent_untouched_r) = parent_regime_metrics
                .get(&rw.label)
                .map(|(d, u)| (d.clone(), u.clone()))
                .ok_or_else(|| anyhow::anyhow!("missing parent regime metrics for label '{}'", rw.label))?;
            regime_inputs.push(RegimeEvalInput {
                label: rw.label.clone(),
                side: rw.side.clone(),
                child_day: child_day_r,
                child_untouched: child_untouched_r,
                parent_day: parent_day_r,
                parent_untouched: parent_untouched_r,
            });
        }

        let (regime_status, regime_rows) =
            classify_from_regime_outcomes(&regime_inputs, min_improvement, holdout_min_improvement, cycle_config.objective);

        // For regime path: use the first regime's day metrics as the primary
        // child_day/child_untouched for the node-metrics side table (so the
        // existing `lineage_node_metrics` row is populated consistently).
        // Fall back to running the main day/baseline scenarios if the regime set
        // happens to be configured without any entries (shouldn't happen here).
        let (child_day, child_untouched) = if let Some(first) = regime_inputs.first() {
            (first.child_day.clone(), first.child_untouched.clone())
        } else {
            (
                paper_tester.run(&child, &cycle_config.day_scenario).await?,
                paper_tester.run(&child, &cycle_config.baseline_scenario).await?,
            )
        };

        // Aggregate delta_sharpe as mean of per-regime deltas.
        // (When regime_set is non-empty, regime_rows is always non-empty;
        // the fallback branch is unreachable but retained for completeness.)
        let delta_sharpe = if !regime_rows.is_empty() {
            regime_rows.iter().map(|r| r.delta_sharpe).sum::<f64>() / regime_rows.len() as f64
        } else {
            // Fallback: unreachable when regime_set is non-empty and validation passes.
            child_day.sharpe - parent_day.sharpe
        };

        // Fix 4: restore the inversion-pair guard for regime-path Active candidates.
        // The multi-window rule filters overfit by requiring improvement across
        // multiple market regimes, but a symmetric noise mutation can still pass
        // every regime window (the forward and reverse diffs both look good in
        // each regime). Run the same inversion check the legacy path uses —
        // scoped only to Active candidates — and downgrade to Rejected on noise.
        let (regime_status, verdict) = match regime_status {
            LineageStatus::Active => {
                progress(CycleProgressEvent::PhaseStarted {
                    session_id: String::new(),
                    cycle_id: cycle_id.to_string(),
                    parent_hash: Some(parent_hash_str.to_string()),
                    phase: Phase::ReverseCheck,
                    detail: "Inversion-pair symmetric-noise check".to_string(),
                });
                let t0 = Instant::now();
                let inv = run_inversion_pair(
                    parent_strategy,
                    &diff,
                    paper_tester,
                    &cycle_config.day_scenario,
                    &cycle_config.baseline_scenario,
                )
                .await?;
                progress(CycleProgressEvent::PhaseFinished {
                    session_id: String::new(),
                    cycle_id: cycle_id.to_string(),
                    parent_hash: Some(parent_hash_str.to_string()),
                    phase: Phase::ReverseCheck,
                    duration_ms: t0.elapsed().as_millis() as u64,
                });
                if inv.symmetric_noise {
                    (
                        LineageStatus::Rejected,
                        GateVerdict::Fail {
                            reason: "inversion-pair symmetric noise".to_string(),
                        },
                    )
                } else {
                    (LineageStatus::Active, GateVerdict::Pass)
                }
            }
            LineageStatus::Quarantined => (
                LineageStatus::Quarantined,
                GateVerdict::Fail {
                    reason: "regime-matrix: passes some but not bull+bear".to_string(),
                },
            ),
            LineageStatus::Rejected => (
                LineageStatus::Rejected,
                GateVerdict::Fail {
                    reason: "regime-matrix: fails all regimes".to_string(),
                },
            ),
        };

        return Ok(MutationOutcome {
            child,
            diff,
            child_hash,
            verdict,
            status: regime_status,
            delta_sharpe,
            child_day,
            child_untouched,
            regime_rows,
            // Regime-matrix path: no single day/holdout pair is the gate gate.
            gate_scores: None,
            // Regime-matrix path runs several evals across windows — no single
            // definitive eval run to nest under the experiment (WS-11b).
            eval_run_id: None,
        });
    }

    // ── Legacy / single-pair / scenario_pool path ────────────────────────────
    // B19: evaluate the child on the caller-selected `sampled_day` /
    // `sampled_baseline` pair (the single cycle pair on the legacy path; the
    // round-robin-sampled pair on the scenario_pool path). The `parent_day` /
    // `parent_untouched` metrics passed in were computed on this SAME pair, so
    // parent and child remain directly comparable in the gate.
    progress(CycleProgressEvent::PhaseStarted {
        session_id: String::new(),
        cycle_id: cycle_id.to_string(),
        parent_hash: Some(parent_hash_str.to_string()),
        phase: Phase::EvalDayWindow,
        detail: "Day-window backtest".to_string(),
    });
    let t0 = Instant::now();
    // WS-11b: the candidate's PRIMARY day-window eval. Use `run_with_run_id` so
    // the persisted eval `Run.id` flows onto `MutationGated` for the frontend
    // experiment → eval-run nesting. `None` when the runner doesn't surface a
    // run id (test stubs) — the experiment row then renders without the node.
    let (child_day, eval_run_id) = paper_tester.run_with_run_id(&child, sampled_day).await?;
    progress(CycleProgressEvent::PhaseFinished {
        session_id: String::new(),
        cycle_id: cycle_id.to_string(),
        parent_hash: Some(parent_hash_str.to_string()),
        phase: Phase::EvalDayWindow,
        duration_ms: t0.elapsed().as_millis() as u64,
    });

    progress(CycleProgressEvent::PhaseStarted {
        session_id: String::new(),
        cycle_id: cycle_id.to_string(),
        parent_hash: Some(parent_hash_str.to_string()),
        phase: Phase::EvalUntouchedWindow,
        detail: "Untouched-window backtest".to_string(),
    });
    let t0 = Instant::now();
    let child_untouched = paper_tester.run(&child, sampled_baseline).await?;
    progress(CycleProgressEvent::PhaseFinished {
        session_id: String::new(),
        cycle_id: cycle_id.to_string(),
        parent_hash: Some(parent_hash_str.to_string()),
        phase: Phase::EvalUntouchedWindow,
        duration_ms: t0.elapsed().as_millis() as u64,
    });
    let raw_verdict = gate_check(
        parent_day,
        &child_day,
        parent_untouched,
        &child_untouched,
        min_improvement,
        holdout_min_improvement,
        cycle_config.objective,
    );
    let delta_sharpe = child_day.sharpe - parent_day.sharpe;

    let (verdict, status) = if matches!(raw_verdict, GateVerdict::Pass) {
        progress(CycleProgressEvent::PhaseStarted {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: Some(parent_hash_str.to_string()),
            phase: Phase::ReverseCheck,
            detail: "Inversion-pair symmetric-noise check".to_string(),
        });
        let t0 = Instant::now();
        let inv = run_inversion_pair(
            parent_strategy,
            &diff,
            paper_tester,
            sampled_day,
            sampled_baseline,
        )
        .await?;
        progress(CycleProgressEvent::PhaseFinished {
            session_id: String::new(),
            cycle_id: cycle_id.to_string(),
            parent_hash: Some(parent_hash_str.to_string()),
            phase: Phase::ReverseCheck,
            duration_ms: t0.elapsed().as_millis() as u64,
        });
        if inv.symmetric_noise {
            (
                GateVerdict::Fail {
                    reason: "inversion-pair symmetric noise".to_string(),
                },
                LineageStatus::Rejected,
            )
        } else {
            (GateVerdict::Pass, LineageStatus::Active)
        }
    } else {
        (raw_verdict, LineageStatus::Rejected)
    };

    // Capture numeric gate scores for persistence to autooptimizer_gate_records.
    let obj = cycle_config.objective;
    let parent_day_score = obj.oriented_value(parent_day);
    let child_day_score = obj.oriented_value(&child_day);
    let parent_holdout_score = obj.oriented_value(parent_untouched);
    let child_holdout_score = obj.oriented_value(&child_untouched);
    let drawdown_ratio = {
        let parent_worst = parent_day
            .max_drawdown_pct
            .abs()
            .max(parent_untouched.max_drawdown_pct.abs());
        let child_worst = child_day
            .max_drawdown_pct
            .abs()
            .max(child_untouched.max_drawdown_pct.abs());
        if parent_worst > 0.0 {
            Some(child_worst / parent_worst)
        } else {
            None
        }
    };
    // Random-baseline edge metrics (informational; never gating). Memoized per
    // (training window, direction) so the extra backtest runs at most once per
    // distinct training window. Uses the parent's structure (risk sizing, filters)
    // with random, direction-restricted decisions. B19: keyed on the SAMPLED day
    // scenario so the child's edge is measured against a baseline on the same
    // window it was scored on (the cache key already keys on scenario id, so
    // distinct pool pairs get distinct baselines).
    let baseline_score = random_baseline_score(
        paper_tester,
        parent_strategy,
        sampled_day,
        baseline_direction,
        obj,
        baseline_cache,
    )
    .await;
    let (edge_over_random, parent_edge, edge_delta) = if baseline_score.is_finite() {
        let eor = child_day_score - baseline_score;
        let pe = parent_day_score - baseline_score;
        (Some(eor), Some(pe), Some(eor - pe))
    } else {
        (None, None, None)
    };
    let gate_scores = Some(GateScores {
        parent_day_score,
        child_day_score,
        parent_holdout_score,
        child_holdout_score,
        gate_epsilon: min_improvement,
        holdout_epsilon: holdout_min_improvement,
        delta_day: child_day_score - parent_day_score,
        delta_holdout: child_holdout_score - parent_holdout_score,
        drawdown_ratio,
        edge_over_random,
        parent_edge,
        edge_delta,
    });

    Ok(MutationOutcome {
        child,
        diff,
        child_hash,
        verdict,
        status,
        delta_sharpe,
        child_day,
        child_untouched,
        regime_rows: vec![],
        gate_scores,
        eval_run_id,
    })
}

/// Build a (day, baseline) `Scenario` pair for a regime window, cloned and
/// date-patched from the cycle's base `day_scenario`.
fn build_regime_scenario_pair(cycle_config: &CycleConfig, rw: &RegimeWindow) -> Result<(Scenario, Scenario)> {
    use crate::eval::scenario::{BarCachePolicy, RefreshPolicy, TimeWindow};
    use chrono::{NaiveDate, TimeZone, Utc};

    let parse_date = |s: &str| -> Result<chrono::DateTime<Utc>> {
        let nd: NaiveDate = s
            .parse()
            .map_err(|e| anyhow::anyhow!("parse date '{}': {}", s, e))?;
        let midnight = nd
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("could not construct midnight for date '{}'", s))?;
        Ok(Utc.from_utc_datetime(&midnight))
    };

    let day_start = parse_date(&rw.day.start)?;
    let day_end = parse_date(&rw.day.end)?;
    if day_start >= day_end {
        anyhow::bail!("regime '{}' day window is empty or inverted", rw.label);
    }

    let base_start = parse_date(&rw.baseline.start)?;
    let base_end = parse_date(&rw.baseline.end)?;
    if base_start >= base_end {
        anyhow::bail!("regime '{}' baseline window is empty or inverted", rw.label);
    }

    // Day scenario: clone the cycle's base day_scenario, patch window + cache key.
    let mut day_scen = cycle_config.day_scenario.clone();
    day_scen.id = Ulid::new().to_string();
    day_scen.time_window = TimeWindow {
        start: day_start,
        end: day_end,
    };
    day_scen.bar_cache_policy = BarCachePolicy {
        cache_key: format!(
            "regime-{}-day-{}-{}",
            rw.label,
            rw.day.start.replace('-', ""),
            rw.day.end.replace('-', ""),
        ),
        refresh_policy: RefreshPolicy::NeverRefresh,
        data_fetched_at: None,
    };

    // Baseline scenario: clone the day scenario above, patch window + cache key.
    let mut base_scen = day_scen.clone();
    base_scen.id = Ulid::new().to_string();
    base_scen.time_window = TimeWindow {
        start: base_start,
        end: base_end,
    };
    base_scen.bar_cache_policy = BarCachePolicy {
        cache_key: format!(
            "regime-{}-base-{}-{}",
            rw.label,
            rw.baseline.start.replace('-', ""),
            rw.baseline.end.replace('-', ""),
        ),
        refresh_policy: RefreshPolicy::NeverRefresh,
        data_fetched_at: None,
    };

    Ok((day_scen, base_scen))
}

/// Returns `true` when the new outcome status is Rejected or Quarantined —
/// i.e. a worse outcome that must not overwrite an existing Active node in the
/// `INSERT OR REPLACE`-keyed `lineage_nodes` table.
fn should_preserve_active_collision(new_status: &LineageStatus) -> bool {
    matches!(new_status, LineageStatus::Rejected | LineageStatus::Quarantined)
}

/// Build and atomically persist a lineage node together with its per-regime
/// audit rows (if any).  Returns the node that was actually written to the DB
/// and its **resolved** [`LineageStatus`] — which may differ from
/// `outcome.status` when the collision-guard preserves an existing Active node.
///
/// Fix 1 (MAJOR): node insert and regime-results insert are wrapped in a single
/// SQLite transaction so a regime-insert failure cannot leave a node without its
/// audit rows.
///
/// Fix 2 (MAJOR): the resolved status is returned so callers can route the SSE
/// event and result buckets from what the DB actually persisted, not from the
/// pre-persistence derived status.
async fn build_and_insert_node(
    pool: &SqlitePool,
    strategy_blob_store: &BlobStore,
    outcome: &MutationOutcome,
    parent_node: &LineageNode,
    cycle_id: &str,
) -> Result<(LineageNode, LineageStatus)> {
    let store = LineageStore::new(pool.clone());
    // F33: record this cycle's evaluation edge to the candidate up-front (before
    // the F12 active-node guard below can early-return), so a cycle that
    // re-derives an existing candidate still attributes it to itself in the
    // run-detail surface — even though the content-addressed `lineage_nodes` row
    // belongs to whichever cycle wrote it first.
    if let Err(e) = crate::autooptimizer::lineage::record_cycle_node_eval(
        pool,
        cycle_id,
        &outcome.child_hash.to_hex(),
        &Utc::now().to_rfc3339(),
    )
    .await
    {
        tracing::warn!(cycle_id, error = %e, "failed to record cycle_node_evaluations edge");
    }
    // F12: `lineage_nodes` uses `INSERT OR REPLACE` keyed on `bundle_hash`. If a
    // non-active candidate (Rejected *or* Quarantined) hashes to an already-*active*
    // node (a re-derivation of a known-good strategy), replacing it would downgrade
    // the active node and poison future re-runs/parent selection. Keep the active
    // node and return it untouched rather than overwriting it with a worse outcome.
    // Return the *Active* status so downstream SSE/buckets reflect what the DB holds.
    if should_preserve_active_collision(&outcome.status) {
        if let Some(existing) = store.get(&outcome.child_hash).await? {
            if existing.status == LineageStatus::Active {
                tracing::debug!(
                    cycle_id,
                    child_hash = %outcome.child_hash.to_hex(),
                    new_status = ?outcome.status,
                    "candidate collides with an existing active node — keeping the active node",
                );
                let resolved_status = existing.status.clone();
                return Ok((existing, resolved_status));
            }
        }
    }
    let node = LineageNode {
        bundle_hash: outcome.child_hash,
        parent_hash: Some(parent_node.bundle_hash),
        gate_verdict: outcome.verdict.clone(),
        status: outcome.status.clone(),
        cycle_id: Some(cycle_id.to_string()),
        created_at: Utc::now(),
        diversity_score: None,
    };

    // Fix 1: wrap node insert + regime rows in a single transaction so a
    // failure in regime-results insert cannot leave a node without its audit rows.
    if !outcome.regime_rows.is_empty() {
        let created_at = Utc::now().to_rfc3339();
        let mut tx = pool.begin().await.context("begin node+regime tx")?;
        sqlx::query(
            "INSERT OR REPLACE INTO lineage_nodes \
             (bundle_hash, parent_hash, gate_verdict, status, cycle_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(node.bundle_hash.to_hex())
        .bind(node.parent_hash.as_ref().map(|h| h.to_hex()))
        .bind(node.gate_verdict.as_str())
        .bind(node.status.as_str())
        .bind(&node.cycle_id)
        .bind(node.created_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .context("insert lineage_node (tx)")?;
        insert_regime_results(
            &mut tx,
            &outcome.child_hash.to_hex(),
            &outcome.regime_rows,
            &created_at,
        )
        .await
        .with_context(|| {
            format!(
                "failed to persist regime results for {}",
                outcome.child_hash.to_hex()
            )
        })?;
        tx.commit().await.context("commit node+regime tx")?;
    } else {
        store.insert(&node).await?;
    }

    // F13: persist the candidate strategy blob so `GET /api/autooptimizer/blob/
    // :hash` resolves the child (the cycle previously wrote only the parent
    // root), enabling "candidate diff via blob" — parent vs child — in the
    // run-detail surface.
    if let Ok(child_json) = serde_json::to_value(&outcome.child) {
        if let Err(e) = strategy_blob_store.put_json(&child_json).await {
            tracing::warn!(child_hash = %outcome.child_hash.to_hex(), "failed to persist candidate blob: {e}");
        }
    }

    // F13: persist per-candidate backtest metrics for the run-detail surface.
    persist_node_metrics(
        pool,
        &outcome.child_hash,
        &outcome.child_day,
        &outcome.child_untouched,
    )
    .await;

    let resolved_status = node.status.clone();
    Ok((node, resolved_status))
}

/// F13: store the candidate's day + held-out `MetricsSummary` in the
/// `lineage_node_metrics` side table. Best-effort — auxiliary detail must never
/// abort the cycle (e.g. a pre-F13 DB that hasn't run `ensure_lineage_schema`);
/// production provisions the table on `ApiContext::open` / CLI db-open.
async fn persist_node_metrics(
    pool: &SqlitePool,
    child_hash: &ContentHash,
    day: &MetricsSummary,
    untouched: &MetricsSummary,
) {
    let day_json = serde_json::to_string(day).unwrap_or_else(|_| "null".to_string());
    let untouched_json = serde_json::to_string(untouched).unwrap_or_else(|_| "null".to_string());
    if let Err(e) = sqlx::query(
        "INSERT OR REPLACE INTO lineage_node_metrics \
         (bundle_hash, metrics_day_json, metrics_untouched_json) VALUES (?, ?, ?)",
    )
    .bind(child_hash.to_hex())
    .bind(day_json)
    .bind(untouched_json)
    .execute(pool)
    .await
    {
        tracing::warn!(child_hash = %child_hash.to_hex(), "failed to persist candidate metrics: {e}");
    }
}

/// F13: persist the per-cycle honesty-check (canary) outcome. Best-effort for
/// the same reason as [`persist_node_metrics`].
async fn persist_honesty_check(pool: &SqlitePool, cycle_id: &str, check: &HonestyCheckResult) {
    if let Err(e) = sqlx::query(
        "INSERT OR REPLACE INTO cycle_honesty_checks \
         (cycle_id, passed, sabotage_variant, message, gate_verdict, parent_hash, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(cycle_id)
    .bind(check.passed_check as i64)
    .bind(&check.sabotage_variant)
    .bind(&check.message)
    .bind(check.gate_verdict.as_str())
    .bind(check.parent_hash.to_hex())
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await
    {
        tracing::warn!(cycle_id, "failed to persist honesty check: {e}");
    }
}
fn gate_check(
    parent_day: &MetricsSummary,
    child_day: &MetricsSummary,
    parent_untouched: &MetricsSummary,
    child_untouched: &MetricsSummary,
    min_improvement: f64,
    holdout_min_improvement: f64,
    objective: Objective,
) -> GateVerdict {
    evaluate(&GateInput {
        parent_day_metrics: parent_day.clone(),
        child_day_metrics: child_day.clone(),
        parent_untouched_metrics: parent_untouched.clone(),
        child_untouched_metrics: child_untouched.clone(),
        min_improvement,
        holdout_min_improvement,
        objective,
    })
}

// ── Phase 2: regime-matrix pure helper ───────────────────────────────────────

/// Inputs to the per-regime evaluation aggregation.
///
/// All fields are the already-computed metric snapshots for one regime window;
/// the helper never performs I/O.
pub struct RegimeEvalInput {
    pub label: String,
    pub side: RegimeSide,
    pub child_day: MetricsSummary,
    pub child_untouched: MetricsSummary,
    pub parent_day: MetricsSummary,
    pub parent_untouched: MetricsSummary,
}

/// Pure, side-effect-free aggregation of per-regime gate results.
///
/// For each input it calls `gate::evaluate` with `min_improvement`, computes
/// `delta_sharpe`, builds a [`RegimeResultRow`], then hands the
/// `(RegimeSide, GateVerdict)` pairs to [`aggregate_regime_verdicts`] to
/// derive the overall [`LineageStatus`].
///
/// Returns `(status, rows)`.  The caller runs the backtests to supply the
/// `RegimeEvalInput`s; this function is deterministic given those inputs.
pub fn classify_from_regime_outcomes(
    regimes: &[RegimeEvalInput],
    min_improvement: f64,
    holdout_min_improvement: f64,
    objective: Objective,
) -> (LineageStatus, Vec<RegimeResultRow>) {
    let mut side_verdict_pairs: Vec<(RegimeSide, GateVerdict)> = Vec::with_capacity(regimes.len());
    let mut rows: Vec<RegimeResultRow> = Vec::with_capacity(regimes.len());

    for r in regimes {
        let verdict = evaluate(&GateInput {
            parent_day_metrics: r.parent_day.clone(),
            child_day_metrics: r.child_day.clone(),
            parent_untouched_metrics: r.parent_untouched.clone(),
            child_untouched_metrics: r.child_untouched.clone(),
            min_improvement,
            holdout_min_improvement,
            objective,
        });
        let delta_sharpe = r.child_day.sharpe - r.parent_day.sharpe;
        rows.push(RegimeResultRow {
            regime_label: r.label.clone(),
            side: r.side.clone(),
            metrics_day: r.child_day.clone(),
            metrics_untouched: r.child_untouched.clone(),
            delta_sharpe,
            verdict: verdict.as_str(),
        });
        side_verdict_pairs.push((r.side.clone(), verdict));
    }

    let status = aggregate_regime_verdicts(&side_verdict_pairs);
    (status, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autooptimizer::gate::GateVerdict;

    fn active_node() -> LineageNode {
        LineageNode {
            bundle_hash: ContentHash::of_bytes(b"candidate"),
            parent_hash: None,
            gate_verdict: GateVerdict::Pass,
            status: LineageStatus::Active,
            cycle_id: Some("c".into()),
            created_at: Utc::now(),
            diversity_score: None,
        }
    }

    #[test]
    fn honesty_check_skipped_when_no_candidate_gated() {
        // run-7: no gated candidate (both empty) => not warranted (skip the canary).
        assert!(!honesty_check_warranted(&[], &[], &[]));
    }

    #[test]
    fn honesty_check_runs_when_a_candidate_was_gated() {
        let kept = vec![active_node()];
        assert!(
            honesty_check_warranted(&kept, &[], &[]),
            "a kept candidate warrants the check"
        );
        assert!(
            honesty_check_warranted(&[], &kept, &[]),
            "a suspect candidate warrants the check"
        );
        assert!(
            honesty_check_warranted(&[], &[], &kept),
            "a rejected candidate warrants the check"
        );
    }

    /// Fix 1: the collision-guard predicate must return true for both Rejected
    /// and Quarantined (not just Rejected), and false for Active.
    #[test]
    fn should_preserve_active_collision_covers_rejected_and_quarantined() {
        assert!(
            should_preserve_active_collision(&LineageStatus::Rejected),
            "Rejected must trigger the guard"
        );
        assert!(
            should_preserve_active_collision(&LineageStatus::Quarantined),
            "Quarantined must trigger the guard — a Quarantined insert must not overwrite an Active node"
        );
        assert!(
            !should_preserve_active_collision(&LineageStatus::Active),
            "Active should NOT trigger the guard — a new Active outcome may overwrite an older Active"
        );
    }

    /// Bull passes, BearOrShock fails → aggregate is Quarantined (Suspect).
    /// Two rows come back with the expected labels and sides.
    #[test]
    fn classify_bull_pass_bear_fail_yields_quarantined() {
        let make_metrics = |sharpe: f64| MetricsSummary {
            sharpe,
            total_return_pct: 0.0,
            max_drawdown_pct: 5.0,
            win_rate: 0.5,
            n_trades: 10,
            n_decisions: 20,
            inference_cost_quote_total: None,
            net_return_pct: None,
            baselines: None,
        };

        // Bull: child sharpe 1.2 vs parent 1.0 → Δ = 0.2 > 0.1 → Pass
        // BearOrShock: child sharpe 0.3 vs parent 0.5 → Δ = -0.2 < 0.1 → Fail
        let regimes = vec![
            RegimeEvalInput {
                label: "bull_2024".to_string(),
                side: RegimeSide::Bull,
                parent_day: make_metrics(1.0),
                parent_untouched: make_metrics(1.0),
                child_day: make_metrics(1.2),
                child_untouched: make_metrics(1.2),
            },
            RegimeEvalInput {
                label: "bear_2022".to_string(),
                side: RegimeSide::BearOrShock,
                parent_day: make_metrics(0.5),
                parent_untouched: make_metrics(0.5),
                child_day: make_metrics(0.3),
                child_untouched: make_metrics(0.3),
            },
        ];

        let (status, rows) = classify_from_regime_outcomes(&regimes, 0.1, 0.1, Objective::Sharpe);

        assert_eq!(
            status,
            LineageStatus::Quarantined,
            "expected Quarantined (Suspect)"
        );
        assert_eq!(rows.len(), 2, "expected exactly 2 regime rows");

        // Rows come back in input order.
        let bull_row = rows
            .iter()
            .find(|r| r.regime_label == "bull_2024")
            .expect("bull row missing");
        let bear_row = rows
            .iter()
            .find(|r| r.regime_label == "bear_2022")
            .expect("bear row missing");

        assert!(matches!(bull_row.side, RegimeSide::Bull));
        assert_eq!(bull_row.verdict, "passed");
        assert!((bull_row.delta_sharpe - 0.2).abs() < 1e-9, "bull Δsharpe");

        assert!(matches!(bear_row.side, RegimeSide::BearOrShock));
        // GateVerdict::Fail.as_str() → "rejected:<reason>"
        assert!(
            bear_row.verdict.starts_with("rejected:"),
            "bear verdict should be rejected, got: {}",
            bear_row.verdict
        );
        assert!((bear_row.delta_sharpe - (-0.2)).abs() < 1e-9, "bear Δsharpe");
    }

    // ── B19: round-robin scenario_pair selection ──────────────────────────────

    use crate::autooptimizer::config::DayWindow;
    use crate::autooptimizer::scenario_synthesis::synthesize_optimizer_day_scenario;

    fn pool_scenario(label: &str, year: i32) -> Scenario {
        let mut s = synthesize_optimizer_day_scenario(
            &DayWindow {
                start: chrono::NaiveDate::from_ymd_opt(year, 1, 1).unwrap(),
                end: chrono::NaiveDate::from_ymd_opt(year, 3, 1).unwrap(),
            },
            60,
            "test",
        );
        // Make the id deterministic per label so assertions don't depend on ULID.
        s.id = format!("day-{label}");
        s.display_name = label.to_string();
        s
    }

    #[test]
    fn select_scenario_pair_empty_pool_always_returns_fallback() {
        // Back-compat: an empty pool must return the single fallback pair for
        // EVERY candidate index — identical to the legacy single-pair behavior.
        let fb_day = pool_scenario("fallback-day", 2025);
        let fb_base = pool_scenario("fallback-base", 2026);
        let pool: Vec<(Scenario, Scenario)> = vec![];
        for i in 0..10 {
            let (d, b) = select_scenario_pair(&pool, (&fb_day, &fb_base), i);
            assert_eq!(d.id, "day-fallback-day", "idx {i} must use fallback day");
            assert_eq!(b.id, "day-fallback-base", "idx {i} must use fallback baseline");
        }
    }

    #[test]
    fn select_scenario_pair_round_robins_pool_by_index_modulo_len() {
        // K = 3 pairs, N = 7 candidates → candidate i uses pair i % 3.
        let fb_day = pool_scenario("fallback-day", 2025);
        let fb_base = pool_scenario("fallback-base", 2026);
        let pool: Vec<(Scenario, Scenario)> = vec![
            (pool_scenario("p0-day", 2020), pool_scenario("p0-base", 2021)),
            (pool_scenario("p1-day", 2022), pool_scenario("p1-base", 2023)),
            (pool_scenario("p2-day", 2024), pool_scenario("p2-base", 2025)),
        ];
        let expected_day = ["day-p0-day", "day-p1-day", "day-p2-day"];
        let expected_base = ["day-p0-base", "day-p1-base", "day-p2-base"];
        for i in 0..7usize {
            let (d, b) = select_scenario_pair(&pool, (&fb_day, &fb_base), i);
            assert_eq!(
                d.id,
                expected_day[i % 3],
                "candidate {i} must select pool day pair {}",
                i % 3
            );
            assert_eq!(
                b.id,
                expected_base[i % 3],
                "candidate {i} must select pool baseline pair {}",
                i % 3
            );
            // The fallback must NOT leak in when the pool is non-empty.
            assert_ne!(d.id, "day-fallback-day", "candidate {i} must not use fallback");
        }
    }

    #[test]
    fn select_scenario_pair_pair_is_self_consistent_for_a_candidate() {
        // Comparability invariant (pure level): the day and baseline returned for a
        // given candidate come from the SAME pool entry — so the parent baseline
        // computed on this same pair is directly comparable to the child. We assert
        // the returned day/baseline always belong to the same entry index.
        let fb_day = pool_scenario("fallback-day", 2025);
        let fb_base = pool_scenario("fallback-base", 2026);
        let pool: Vec<(Scenario, Scenario)> = vec![
            (pool_scenario("p0-day", 2020), pool_scenario("p0-base", 2021)),
            (pool_scenario("p1-day", 2022), pool_scenario("p1-base", 2023)),
        ];
        for i in 0..6usize {
            let (d, b) = select_scenario_pair(&pool, (&fb_day, &fb_base), i);
            let idx = i % 2;
            // day suffix and baseline suffix must share the same pair index.
            assert!(d.id.starts_with(&format!("day-p{idx}-")));
            assert!(b.id.starts_with(&format!("day-p{idx}-")));
        }
    }
}
