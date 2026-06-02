//! Evening-cycle orchestrator — AR-2 Task 9.

use std::collections::HashMap;

use anyhow::{bail, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use ulid::Ulid;
use xvision_observability::BlobStore;

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
}

struct MutationOutcome {
    child: Strategy,
    diff: MutationDiff,
    child_hash: ContentHash,
    verdict: GateVerdict,
    status: LineageStatus,
    delta_sharpe: f64,
}

pub async fn run_evening_cycle(
    pool: &SqlitePool,
    _blob_store: &BlobStore,
    config: &AutoOptimizerConfig,
    cycle_config: &CycleConfig,
    parent_policy: &ParentPolicy,
    mutator: &Mutator,
    judge: &Judge,
    paper_tester: &dyn PaperTestRunner,
    progress: impl Fn(CycleProgressEvent) + Send + Sync,
    dspy_ctx: Option<&DspyContext>,
) -> Result<CycleResult> {
    let cycle_id = Ulid::new().to_string();
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

    for parent_node in &parents {
        let ph = parent_node.bundle_hash.to_hex();
        if let Some(parent_strategy) = cycle_config.parent_strategies.get(&ph) {
            progress(CycleProgressEvent::ParentSelected {
                cycle_id: cycle_id.clone(),
                parent_hash: ph,
            });
            let (active, rejected) = process_parent_mutations(
                pool,
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
            )
            .await?;
            active_nodes.extend(active);
            rejected_nodes.extend(rejected);
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
    let diversity_score = diversity_decay_for_cycle(pool, &cycle_id).await.unwrap_or(0.0);
    Ok(CycleResult {
        cycle_id,
        active_nodes,
        rejected_nodes,
        honesty_check,
        diversity_score,
        findings_by_node,
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
    });
    Ok(check)
}

async fn process_parent_mutations<F>(
    pool: &SqlitePool,
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
) -> Result<(Vec<LineageNode>, Vec<LineageNode>)>
where
    F: Fn(CycleProgressEvent),
{
    assert!(
        cycle_config.mutations_per_parent <= 64,
        "mutations_per_parent exceeds bound"
    );
    let mut active: Vec<LineageNode> = Vec::new();
    let mut rejected: Vec<LineageNode> = Vec::new();
    let parent_day = paper_tester
        .run(parent_strategy, &cycle_config.day_scenario)
        .await?;
    let parent_untouched = paper_tester
        .run(parent_strategy, &cycle_config.baseline_scenario)
        .await?;

    for _ in 0..cycle_config.mutations_per_parent {
        let diff = match mutator.propose(parent_strategy, config, dsr_prefix).await {
            Ok(d) => d,
            Err(_) => continue,
        };
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
        let node = build_and_insert_node(pool, &outcome, parent_node, cycle_id).await?;
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
    Ok((active, rejected))
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
    let child = apply_mutation_params(parent_strategy, &diff);
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
    })
}

async fn build_and_insert_node(
    pool: &SqlitePool,
    outcome: &MutationOutcome,
    parent_node: &LineageNode,
    cycle_id: &str,
) -> Result<LineageNode> {
    let node = LineageNode {
        bundle_hash: outcome.child_hash,
        parent_hash: Some(parent_node.bundle_hash),
        gate_verdict: outcome.verdict.clone(),
        status: outcome.status.clone(),
        cycle_id: Some(cycle_id.to_string()),
        created_at: Utc::now(),
        diversity_score: None,
    };
    LineageStore::new(pool.clone()).insert(&node).await?;
    Ok(node)
}

fn apply_mutation_params(base: &Strategy, diff: &MutationDiff) -> Strategy {
    assert!(diff.params.len() <= 64, "params count exceeds bound");
    let mut s = base.clone();
    if let serde_json::Value::Object(ref mut map) = s.mechanical_params {
        for change in &diff.params {
            map.insert(change.key.clone(), change.after.clone());
        }
    }
    s
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
