//! Optimizer cycle orchestrator — AR-2 Task 9.

use std::collections::HashMap;

use anyhow::{bail, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use ulid::Ulid;

use crate::autooptimizer::blob_store::BlobStore;
use crate::autooptimizer::canary::{run_honesty_check, HonestyCheckResult};
use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::content_hash::ContentHash;
use crate::autooptimizer::cycle_loosen::effective_min_improvement_for_cycle;
use crate::autooptimizer::diversity::diversity_decay_for_cycle;
use crate::autooptimizer::dspy_flywheel::{handle_cycle_dspy, query_dsr_prefix, DspyContext};
use crate::autooptimizer::eval_adapter::PaperTestRunner;
use crate::autooptimizer::gate::{evaluate, GateInput, GateVerdict};
use crate::autooptimizer::inversion::run_inversion_pair;
use crate::autooptimizer::judge::{run_judge, Finding, Judge};
use crate::autooptimizer::lineage::{LineageNode, LineageStatus, LineageStore};
use crate::autooptimizer::mutator::{MutationDiff, Mutator};
use crate::autooptimizer::mutator_ladder::{record_outcome, record_proposal};
use crate::autooptimizer::parent_policy::{select_parents, ParentPolicy};
use crate::autooptimizer::progress::CycleProgressEvent;
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
}

pub struct CycleResult {
    pub cycle_id: String,
    pub active_nodes: Vec<LineageNode>,
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
            let (active, rejected, nc) = process_parent_mutations(
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
                cancel.as_ref(),
            )
            .await?;
            active_nodes.extend(active);
            rejected_nodes.extend(rejected);
            no_candidate_count += nc;
        }
    }

    let honesty_check = run_cycle_canary(
        &parents,
        cycle_config,
        config,
        mutator,
        paper_tester,
        min_improvement,
        &cycle_id,
        &progress,
    )
    .await?;
    // F13: persist the honesty-check outcome so a historic cycle's detail can
    // report it (it was previously emitted only over SSE / the CLI summary).
    persist_honesty_check(pool, &cycle_id, &honesty_check).await;
    let diversity_score = diversity_decay_for_cycle(pool, &cycle_id).await.unwrap_or(0.0);
    Ok(CycleResult {
        cycle_id,
        active_nodes,
        rejected_nodes,
        honesty_check,
        diversity_score,
        findings_by_node,
        no_candidate_count,
    })
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
    cancel: Option<&std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(Vec<LineageNode>, Vec<LineageNode>, usize)>
where
    F: Fn(CycleProgressEvent),
{
    assert!(
        cycle_config.mutations_per_parent <= 64,
        "mutations_per_parent exceeds bound"
    );
    let mut active: Vec<LineageNode> = Vec::new();
    let mut rejected: Vec<LineageNode> = Vec::new();
    let mut no_candidate_count: usize = 0;
    let parent_day = paper_tester
        .run(parent_strategy, &cycle_config.day_scenario)
        .await?;
    let parent_untouched = paper_tester
        .run(parent_strategy, &cycle_config.baseline_scenario)
        .await?;

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
                .propose(parent_strategy, config, dsr_prefix, exploration_seed)
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
        let is_identity = serde_json::to_value(&candidate)
            .map(|v| ContentHash::of_json(&v) == parent_node.bundle_hash)
            .unwrap_or(false);
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
        )
        .await?;
        progress(CycleProgressEvent::MutationGated {
            cycle_id: cycle_id.to_string(),
            child_hash: outcome.child_hash.to_hex(),
            passed: matches!(outcome.verdict, GateVerdict::Pass),
        });
        let node = build_and_insert_node(pool, strategy_blob_store, &outcome, parent_node, cycle_id).await?;
        record_proposal(
            pool,
            &outcome.child_hash,
            &mutator.provider,
            &mutator.model,
            &cycle_config.prompt_version,
        )
        .await?;
        if outcome.status == LineageStatus::Active {
            record_outcome(pool, &outcome.child_hash, outcome.delta_sharpe).await?;
            let findings = run_judge(judge, parent_strategy, &outcome.child, &outcome.diff, "").await?;
            for f in &findings {
                progress(CycleProgressEvent::JudgeFinding {
                    cycle_id: cycle_id.to_string(),
                    child_hash: outcome.child_hash.to_hex(),
                    severity: format!("{:?}", f.severity),
                    code: f.code.clone(),
                });
            }
            handle_cycle_dspy(config, dspy_ctx, &findings, cycle_id).await?;
            findings_by_node.insert(outcome.child_hash, findings);
            active.push(node);
        } else {
            rejected.push(node);
        }
    }
    Ok((active, rejected, no_candidate_count))
}

async fn gate_and_classify(
    parent_strategy: &Strategy,
    diff: MutationDiff,
    cycle_config: &CycleConfig,
    paper_tester: &dyn PaperTestRunner,
    parent_day: &MetricsSummary,
    parent_untouched: &MetricsSummary,
    min_improvement: f64,
) -> Result<MutationOutcome> {
    let child = diff.apply_to(parent_strategy);
    let child_day = paper_tester.run(&child, &cycle_config.day_scenario).await?;
    let child_untouched = paper_tester.run(&child, &cycle_config.baseline_scenario).await?;
    let raw_verdict = gate_check(
        parent_day,
        &child_day,
        parent_untouched,
        &child_untouched,
        min_improvement,
    );
    let child_hash = ContentHash::of_json(&serde_json::to_value(&child)?);
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
    })
}

async fn build_and_insert_node(
    pool: &SqlitePool,
    strategy_blob_store: &BlobStore,
    outcome: &MutationOutcome,
    parent_node: &LineageNode,
    cycle_id: &str,
) -> Result<LineageNode> {
    let store = LineageStore::new(pool.clone());
    // F12: `lineage_nodes` uses `INSERT OR REPLACE` keyed on `bundle_hash`. If a
    // rejected candidate hashes to an already-*active* node (a re-derivation of
    // a known-good strategy), replacing it would downgrade the active node to
    // rejected and poison future re-runs/parent selection. Keep the active node
    // and return it untouched rather than overwriting it with a rejection.
    if outcome.status == LineageStatus::Rejected {
        if let Some(existing) = store.get(&outcome.child_hash).await? {
            if existing.status == LineageStatus::Active {
                tracing::debug!(
                    cycle_id,
                    child_hash = %outcome.child_hash.to_hex(),
                    "rejected candidate collides with an existing active node — keeping the active node",
                );
                return Ok(existing);
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
    store.insert(&node).await?;

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

    Ok(node)
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
) -> GateVerdict {
    evaluate(&GateInput {
        parent_day_metrics: parent_day.clone(),
        child_day_metrics: child_day.clone(),
        parent_untouched_metrics: parent_untouched.clone(),
        child_untouched_metrics: child_untouched.clone(),
        min_improvement,
    })
}
