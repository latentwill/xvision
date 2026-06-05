//! Optimizer cycle orchestrator — AR-2 Task 9.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use ulid::Ulid;

use crate::autooptimizer::blob_store::BlobStore;
use crate::autooptimizer::canary::{run_honesty_check, HonestyCheckResult};
use crate::autooptimizer::config::{validate_regime_set, AutoOptimizerConfig, RegimeSide, RegimeWindow};
use crate::autooptimizer::content_hash::ContentHash;
use crate::autooptimizer::cycle_loosen::effective_min_improvement_for_cycle;
use crate::autooptimizer::diversity::diversity_decay_for_cycle;
use crate::autooptimizer::dspy_flywheel::{handle_cycle_dspy, query_dsr_prefix, DspyContext};
use crate::autooptimizer::eval_adapter::PaperTestRunner;
use crate::autooptimizer::gate::{aggregate_regime_verdicts, evaluate, GateInput, GateVerdict, Objective};
use crate::autooptimizer::inversion::run_inversion_pair;
use crate::autooptimizer::judge::{run_judge, Finding, Judge};
use crate::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore};
use crate::autooptimizer::mutator::{MutationDiff, Mutator};
use crate::autooptimizer::mutator_ladder::{record_outcome, record_proposal};
use crate::autooptimizer::parent_policy::{select_parents, ParentPolicy};
use crate::autooptimizer::progress::CycleProgressEvent;
use crate::autooptimizer::regime_results::{insert_regime_results, RegimeResultRow};
use crate::eval::run::MetricsSummary;
use crate::eval::scenario::Scenario;
use crate::strategies::Strategy;

/// Per-cycle configuration.
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
) -> Result<CycleResult> {
    let cycle_id = cycle_id_override.unwrap_or_else(|| Ulid::new().to_string());
    let min_improvement =
        effective_min_improvement_for_cycle(pool, config, 0, cycle_config.sustained_no_pass_cycles)
            .await?
            .effective_min_improvement;

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
        cycle_id: cycle_id.clone(),
        parent_count: parents.len(),
    });

    let mut active_nodes: Vec<LineageNode> = Vec::new();
    let mut suspect_nodes: Vec<LineageNode> = Vec::new();
    let mut rejected_nodes: Vec<LineageNode> = Vec::new();
    let mut findings_by_node: HashMap<ContentHash, Vec<Finding>> = HashMap::new();
    let mut no_candidate_count: usize = 0;

    let is_cancelled = || {
        cancel
            .as_ref()
            .is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed))
    };

    for parent_node in &parents {
        if is_cancelled() {
            break;
        }
        let ph = parent_node.bundle_hash.to_hex();
        if let Some(parent_strategy) = cycle_config.parent_strategies.get(&ph) {
            progress(CycleProgressEvent::ParentSelected {
                cycle_id: cycle_id.clone(),
                parent_hash: ph,
            });
            let (active, suspect, rejected, nc) = process_parent_mutations(
                pool,
                strategy_blob_store,
                parent_node,
                parent_strategy,
                &cycle_id,
                min_improvement,
                cycle_config,
                config,
                mutator,
                judge,
                paper_tester,
                &progress,
                &mut findings_by_node,
                dsr_prefix.as_deref(),
                dspy_ctx,
                memory,
                cancel.as_ref(),
            )
            .await?;
            active_nodes.extend(active);
            suspect_nodes.extend(suspect);
            rejected_nodes.extend(rejected);
            no_candidate_count += nc;
        }
    }

    // run-7: when this cycle produced NO gated candidate (every parent yielded
    // no_candidate, so nothing was backtested), running the sabotage/canary evals
    // is pure waste — there is no candidate whose gating to sanity-check. Skip it
    // and record a skipped result, mirroring the existing "no canary parent"
    // skipped representation (sabotage_variant "none" + a skipped message).
    let honesty_check = if honesty_check_warranted(&active_nodes, &suspect_nodes, &rejected_nodes) {
        run_cycle_canary(
            &parents,
            cycle_config,
            config,
            mutator,
            paper_tester,
            min_improvement,
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
            cycle_id: cycle_id.clone(),
            passed: skipped.passed_check,
            sabotage_variant: skipped.sabotage_variant.clone(),
            message: skipped.message.clone(),
        });
        skipped
    };
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
    })
}

/// The honesty check (sabotage canary) only adds value when the cycle actually
/// gated a candidate. When every parent yielded `no_candidate` — so nothing was
/// backtested — there is no gating to sanity-check, and running the canary evals
/// is pure waste (run-7 finding). Returns `true` iff at least one candidate was
/// gated (kept or rejected) this cycle.
fn honesty_check_warranted(active: &[LineageNode], suspect: &[LineageNode], rejected: &[LineageNode]) -> bool {
    !active.is_empty() || !suspect.is_empty() || !rejected.is_empty()
}

async fn run_cycle_canary<F>(
    parents: &[LineageNode],
    cycle_config: &CycleConfig,
    config: &AutoOptimizerConfig,
    mutator: &Mutator,
    paper_tester: &dyn PaperTestRunner,
    min_improvement: f64,
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
    let obj = cycle_config.objective;
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
            objective: obj,
        },
        &cycle_config.day_scenario,
        &cycle_config.baseline_scenario,
        config,
        cycle_config.sabotage_seed,
    )
    .await?;
    progress(CycleProgressEvent::HonestyCheckRun {
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

async fn process_parent_mutations<F>(
    pool: &SqlitePool,
    strategy_blob_store: &BlobStore,
    parent_node: &LineageNode,
    parent_strategy: &Strategy,
    cycle_id: &str,
    min_improvement: f64,
    cycle_config: &CycleConfig,
    config: &AutoOptimizerConfig,
    mutator: &Mutator,
    judge: &Judge,
    paper_tester: &dyn PaperTestRunner,
    progress: &F,
    findings_by_node: &mut HashMap<ContentHash, Vec<Finding>>,
    dsr_prefix: Option<&str>,
    dspy_ctx: Option<&DspyContext>,
    memory: Option<&crate::agent::memory_recorder::MemoryRecorder>,
    cancel: Option<&std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(Vec<LineageNode>, Vec<LineageNode>, Vec<LineageNode>, usize)>
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

    // Fix 6: Only backtest the parent on the legacy day/baseline scenarios when
    // regime_set is empty. When regime_set is non-empty, the legacy day+baseline
    // metrics are never used for classification — only per-regime backtests drive
    // the gate. Skipping these runs avoids wasted backtests.
    let (parent_day, parent_untouched) = if cycle_config.regime_set.is_empty() {
        let pd = paper_tester
            .run(parent_strategy, &cycle_config.day_scenario)
            .await?;
        let pu = paper_tester
            .run(parent_strategy, &cycle_config.baseline_scenario)
            .await?;
        (pd, pu)
    } else {
        (MetricsSummary::default(), MetricsSummary::default())
    };

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

    for mutation_idx in 0..cycle_config.mutations_per_parent {
        // F28: stop launching further candidates once the operator cancels.
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
        let diff = if config.tournament_enabled {
            use crate::autooptimizer::tournament::TournamentRunner;
            let runner = TournamentRunner::from_mutator(mutator);
            match runner.run_tournament(parent_strategy, config).await {
                Ok(r) if r.incumbent_wins => {
                    no_candidate_count += 1;
                    progress(CycleProgressEvent::NoCandidate {
                        cycle_id: cycle_id.to_string(),
                        parent_hash: parent_node.bundle_hash.to_hex(),
                        reason: "tournament incumbent retained (no challenger improved on the parent)"
                            .to_string(),
                    });
                    continue;
                }
                Ok(r) => r.winner_diff,
                Err(e) => {
                    no_candidate_count += 1;
                    progress(CycleProgressEvent::NoCandidate {
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
                    mutation_memory_context.as_deref(),
                    &avoid,
                )
                .await
            {
                Ok(d) => d,
                Err(e) => {
                    // The mutator exhausted its retries without a distinct,
                    // valid diff (e.g. every attempt was an identity/no-op —
                    // F14). Surface it rather than exiting the iteration silently.
                    no_candidate_count += 1;
                    progress(CycleProgressEvent::NoCandidate {
                        cycle_id: cycle_id.to_string(),
                        parent_hash: parent_node.bundle_hash.to_hex(),
                        reason: format!("experiment writer produced no usable candidate: {e}"),
                    });
                    continue;
                }
            }
        };
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
                    cycle_id: cycle_id.to_string(),
                    parent_hash: parent_node.bundle_hash.to_hex(),
                    reason: "experiment writer re-derived a candidate already evaluated on this parent"
                        .to_string(),
                });
                continue;
            }
        }
        progress(CycleProgressEvent::MutationProposed {
            cycle_id: cycle_id.to_string(),
            parent_hash: parent_node.bundle_hash.to_hex(),
        });
        let outcome = gate_and_classify(
            parent_strategy,
            diff,
            cycle_config,
            paper_tester,
            &parent_day,
            &parent_untouched,
            min_improvement,
            &parent_regime_metrics,
        )
        .await?;
        // Fix 1+2: build_and_insert_node now atomically writes node + regime rows
        // and returns the *resolved* status (which may be Active when the
        // collision-guard preserves an existing active node).  Emit the SSE event
        // and route the result bucket from the resolved status, not outcome.status.
        let (node, resolved_status) = build_and_insert_node(pool, strategy_blob_store, &outcome, parent_node, cycle_id).await?;
        let outcome_str = match &resolved_status {
            LineageStatus::Active => "kept",
            LineageStatus::Quarantined => "suspect",
            LineageStatus::Rejected => "dropped",
        };
        progress(CycleProgressEvent::MutationGated {
            cycle_id: cycle_id.to_string(),
            child_hash: outcome.child_hash.to_hex(),
            passed: matches!(outcome.verdict, GateVerdict::Pass),
            outcome: outcome_str.to_string(),
        });
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
                let findings = run_judge(
                    judge,
                    parent_strategy,
                    &outcome.child,
                    &outcome.diff,
                    "",
                    memory,
                    Some(cycle_config.day_scenario.time_window.start),
                )
                .await?;
                for f in &findings {
                    progress(CycleProgressEvent::JudgeFinding {
                        cycle_id: cycle_id.to_string(),
                        child_hash: outcome.child_hash.to_hex(),
                        severity: format!("{:?}", f.severity),
                        code: f.code.clone(),
                    });
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
                handle_cycle_dspy(config, dspy_ctx, &findings, cycle_id).await?;
                findings_by_node.insert(outcome.child_hash, findings);
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
    Ok((active, suspect, rejected, no_candidate_count))
}

async fn gate_and_classify(
    parent_strategy: &Strategy,
    diff: MutationDiff,
    cycle_config: &CycleConfig,
    paper_tester: &dyn PaperTestRunner,
    parent_day: &MetricsSummary,
    parent_untouched: &MetricsSummary,
    min_improvement: f64,
    // Per-regime parent metrics (label → (day, untouched)), pre-computed by
    // `process_parent_mutations` so each parent is evaluated only once per
    // regime window across all its mutations.
    parent_regime_metrics: &HashMap<String, (MetricsSummary, MetricsSummary)>,
) -> Result<MutationOutcome> {
    let child = diff.apply_to(parent_strategy);
    let child_hash = ContentHash::of_json(&serde_json::to_value(&child)?);

    // ── Phase 2: regime-matrix path ──────────────────────────────────────────
    if !cycle_config.regime_set.is_empty() {
        // Build (day, baseline) scenario pairs for every regime window.
        let mut regime_inputs: Vec<RegimeEvalInput> = Vec::with_capacity(cycle_config.regime_set.len());
        for rw in &cycle_config.regime_set {
            let (regime_day_scen, regime_baseline_scen) =
                build_regime_scenario_pair(cycle_config, rw)?;
            let child_day_r = paper_tester.run(&child, &regime_day_scen).await?;
            let child_untouched_r = paper_tester.run(&child, &regime_baseline_scen).await?;
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
            classify_from_regime_outcomes(&regime_inputs, min_improvement, cycle_config.objective);

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
                let inv = run_inversion_pair(
                    parent_strategy,
                    &diff,
                    paper_tester,
                    &cycle_config.day_scenario,
                    &cycle_config.baseline_scenario,
                )
                .await?;
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
        });
    }

    // ── Legacy / empty-regime-set path (UNCHANGED) ───────────────────────────
    let child_day = paper_tester.run(&child, &cycle_config.day_scenario).await?;
    let child_untouched = paper_tester.run(&child, &cycle_config.baseline_scenario).await?;
    let raw_verdict = gate_check(
        parent_day,
        &child_day,
        parent_untouched,
        &child_untouched,
        min_improvement,
        cycle_config.objective,
    );
    let delta_sharpe = child_day.sharpe - parent_day.sharpe;

    let (verdict, status) = if matches!(raw_verdict, GateVerdict::Pass) {
        let inv = run_inversion_pair(
            parent_strategy,
            &diff,
            paper_tester,
            &cycle_config.day_scenario,
            &cycle_config.baseline_scenario,
        )
        .await?;
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
    })
}

/// Build a (day, baseline) `Scenario` pair for a regime window, cloned and
/// date-patched from the cycle's base `day_scenario`.
fn build_regime_scenario_pair(
    cycle_config: &CycleConfig,
    rw: &RegimeWindow,
) -> Result<(Scenario, Scenario)> {
    use chrono::{NaiveDate, TimeZone, Utc};
    use crate::eval::scenario::{BarCachePolicy, RefreshPolicy, TimeWindow};

    let parse_date = |s: &str| -> Result<chrono::DateTime<Utc>> {
        let nd: NaiveDate = s.parse().map_err(|e| anyhow::anyhow!("parse date '{}': {}", s, e))?;
        let midnight = nd.and_hms_opt(0, 0, 0)
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
    day_scen.time_window = TimeWindow { start: day_start, end: day_end };
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
    base_scen.time_window = TimeWindow { start: base_start, end: base_end };
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
        insert_regime_results(&mut tx, &outcome.child_hash.to_hex(), &outcome.regime_rows, &created_at)
            .await
            .with_context(|| format!("failed to persist regime results for {}", outcome.child_hash.to_hex()))?;
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
    objective: Objective,
) -> GateVerdict {
    evaluate(&GateInput {
        parent_day_metrics: parent_day.clone(),
        child_day_metrics: child_day.clone(),
        parent_untouched_metrics: parent_untouched.clone(),
        child_untouched_metrics: child_untouched.clone(),
        min_improvement,
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
        assert!(honesty_check_warranted(&kept, &[], &[]), "a kept candidate warrants the check");
        assert!(honesty_check_warranted(&[], &kept, &[]), "a suspect candidate warrants the check");
        assert!(honesty_check_warranted(&[], &[], &kept), "a rejected candidate warrants the check");
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

        let (status, rows) = classify_from_regime_outcomes(&regimes, 0.1, Objective::Sharpe);

        assert_eq!(status, LineageStatus::Quarantined, "expected Quarantined (Suspect)");
        assert_eq!(rows.len(), 2, "expected exactly 2 regime rows");

        // Rows come back in input order.
        let bull_row = rows.iter().find(|r| r.regime_label == "bull_2024").expect("bull row missing");
        let bear_row = rows.iter().find(|r| r.regime_label == "bear_2022").expect("bear row missing");

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
}
