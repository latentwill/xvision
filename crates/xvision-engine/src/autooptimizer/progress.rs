//! SSE event taxonomy. AR-1 defines the baseline AutoOptimizerEvent; AR-2
//! (cycle orchestrator) adds CycleProgressEvent with operator-friendly labels
//! per the 2026-05-27 terminology lock. AR-3 (dashboard) wires the SSE channel.

use serde::{Deserialize, Serialize};

/// Legacy per-mutation events (AR-1). Kept for backward compatibility with
/// existing subscribers; cycle.rs emits CycleProgressEvent instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutoOptimizerEvent {
    MutationProposed {
        cycle_id: String,
        parent_hash: String,
    },
    MutationEvaluating {
        cycle_id: String,
        child_hash: String,
    },
    MutationCommitted {
        cycle_id: String,
        child_hash: String,
        status: String,
    },
    MutationRejected {
        cycle_id: String,
        child_hash: String,
        reason: String,
    },
    LineageForked {
        cycle_id: String,
        parent_hash: String,
        child_hash: String,
    },
    CanaryOutcome {
        cycle_id: String,
        accepted: bool,
    },
    DiversityUpdated {
        cycle_id: String,
        value: f64,
    },
}

/// Per-cycle orchestrator progress events. Operator-surface labels follow the
/// 2026-05-27 terminology lock: Mutation→Experiment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CycleProgressEvent {
    /// Fired once when the cycle begins. Operator label: "Cycle started".
    CycleStarted { cycle_id: String, parent_count: usize },
    /// Fired once per selected parent. Operator label: "Parent selected".
    ParentSelected { cycle_id: String, parent_hash: String },
    /// Fired when a mutation is proposed for a parent. Operator label: "Experiment proposed".
    MutationProposed { cycle_id: String, parent_hash: String },
    /// Fired when a parent yields no usable candidate this cycle — the mutator
    /// could not produce a distinct, valid experiment within its retry budget
    /// (e.g. every attempt was a no-op/identity diff). Operator label: "No
    /// experiment produced". Distinguishes a genuinely empty cycle from one that
    /// gated a real candidate (F14, QA 2026-06-04).
    NoCandidate {
        cycle_id: String,
        parent_hash: String,
        #[serde(default)]
        reason: String,
    },
    /// Fired after the numeric gate evaluates a child mutation.
    MutationGated {
        cycle_id: String,
        child_hash: String,
        passed: bool,
    },
    /// Fired after the honesty check runs. Operator label: "Honesty check run".
    /// F9: carries the sabotage variant + a human-readable message so the CLI
    /// summary and optimizer panel can render a labeled outcome instead of the
    /// operator having to infer it from raw broker-rule warnings.
    HonestyCheckRun {
        cycle_id: String,
        passed: bool,
        #[serde(default)]
        sabotage_variant: String,
        #[serde(default)]
        message: String,
    },
    /// Fired for each judge finding on an active child. Operator label: "Judge finding".
    JudgeFinding {
        cycle_id: String,
        child_hash: String,
        severity: String,
        code: String,
    },
    /// Fired once the optimizer run task has completed. Operator label: "Optimizer run finished".
    CycleFinished {
        cycle_id: String,
        active_count: usize,
        rejected_count: usize,
    },
}
