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

/// Phase within a cycle where work is being performed. Used in
/// `PhaseStarted` / `PhaseFinished` progress events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    WriterProposing,
    EvalDayWindow,
    EvalUntouchedWindow,
    ReverseCheck,
    GateEvaluating,
    ReviewerRunning,
    HonestyCheck,
}

/// Per-cycle orchestrator progress events. Operator-surface labels follow the
/// 2026-05-27 terminology lock: Mutation→Experiment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CycleProgressEvent {
    /// Fired once when the cycle begins. Operator label: "Cycle started".
    CycleStarted {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        parent_count: usize,
    },
    /// Fired once per selected parent. Operator label: "Parent selected".
    ParentSelected {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        parent_hash: String,
    },
    /// Fired when a mutation is proposed for a parent. Operator label: "Experiment proposed".
    MutationProposed {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        parent_hash: String,
    },
    /// Fired when a parent yields no usable candidate this cycle — the mutator
    /// could not produce a distinct, valid experiment within its retry budget
    /// (e.g. every attempt was a no-op/identity diff). Operator label: "No
    /// experiment produced". Distinguishes a genuinely empty cycle from one that
    /// gated a real candidate (F14, QA 2026-06-04).
    NoCandidate {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        parent_hash: String,
        #[serde(default)]
        reason: String,
    },
    /// Fired after the numeric gate evaluates a child mutation.
    ///
    /// `passed` is kept for backward compatibility with existing consumers
    /// (true = Active, false = Quarantined or Rejected). The additive `outcome`
    /// field carries the precise 3-way result: `"kept"`, `"suspect"`, or
    /// `"dropped"`. New consumers should read `outcome`; legacy consumers that
    /// only read `passed` will see `false` for both Suspect and Rejected as before.
    MutationGated {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        child_hash: String,
        /// Legacy two-way result. `true` = Active (kept). `false` = Quarantined
        /// (suspect) or Rejected (dropped). Kept for back-compat.
        passed: bool,
        /// Three-way result: `"kept"` | `"suspect"` | `"dropped"`.
        #[serde(default)]
        outcome: String,
    },
    /// Fired after the honesty check runs. Operator label: "Honesty check run".
    /// F9: carries the sabotage variant + a human-readable message so the CLI
    /// summary and optimizer panel can render a labeled outcome instead of the
    /// operator having to infer it from raw broker-rule warnings.
    HonestyCheckRun {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        passed: bool,
        #[serde(default)]
        sabotage_variant: String,
        #[serde(default)]
        message: String,
    },
    /// Fired for each judge finding on an active child. Operator label: "Judge finding".
    JudgeFinding {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        child_hash: String,
        severity: String,
        code: String,
    },
    /// Fired once the optimizer run task has completed. Operator label: "Optimizer run finished".
    CycleFinished {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        active_count: usize,
        /// Quarantined (Suspect) nodes — partial-pass across regimes.
        #[serde(default)]
        suspect_count: usize,
        rejected_count: usize,
    },
    /// Fired when a phase boundary begins. Operator label: "Phase started".
    PhaseStarted {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        parent_hash: Option<String>,
        phase: Phase,
        detail: String,
    },
    /// Fired when a phase boundary finishes. Operator label: "Phase finished".
    PhaseFinished {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        parent_hash: Option<String>,
        phase: Phase,
        duration_ms: u64,
    },
    /// Fired when the optimizer session state changes (e.g. running, finished, failed).
    SessionStateChanged { session_id: String, state: String },
    /// Fired after the DSPy flywheel compiles findings into a prompt pattern.
    /// Operator label: "Findings compiled into prompt pattern".
    FlywheelCompiled {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        optimization_run_id: String,
        pattern_id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase enum serializes using snake_case.
    #[test]
    fn test_phase_enum_serde() {
        let v = serde_json::to_value(Phase::WriterProposing).unwrap();
        assert_eq!(v, serde_json::json!("writer_proposing"));

        let v = serde_json::to_value(Phase::EvalDayWindow).unwrap();
        assert_eq!(v, serde_json::json!("eval_day_window"));

        let v = serde_json::to_value(Phase::HonestyCheck).unwrap();
        assert_eq!(v, serde_json::json!("honesty_check"));
    }

    /// FlywheelCompiled must serialize as "flywheel_compiled" (NOT "fly_wheel_compiled").
    #[test]
    fn test_flywheel_compiled_wire_name() {
        let event = CycleProgressEvent::FlywheelCompiled {
            session_id: "s1".into(),
            cycle_id: "c1".into(),
            optimization_run_id: "r1".into(),
            pattern_id: "p1".into(),
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(
            v["type"].as_str().unwrap(),
            "flywheel_compiled",
            "FlywheelCompiled must serialize as 'flywheel_compiled', not 'fly_wheel_compiled'"
        );
    }

    /// Existing events must deserialize from JSON missing session_id, defaulting to "".
    #[test]
    fn test_existing_event_backward_compat() {
        // CycleStarted without session_id field
        let json = r#"{"type":"cycle_started","cycle_id":"c1","parent_count":3}"#;
        let event: CycleProgressEvent = serde_json::from_str(json).unwrap();
        match event {
            CycleProgressEvent::CycleStarted {
                session_id,
                cycle_id,
                parent_count,
            } => {
                assert_eq!(session_id, "", "session_id should default to empty string");
                assert_eq!(cycle_id, "c1");
                assert_eq!(parent_count, 3);
            }
            _ => panic!("wrong variant"),
        }

        // MutationGated without session_id
        let json =
            r#"{"type":"mutation_gated","cycle_id":"c1","child_hash":"abc","passed":true,"outcome":"kept"}"#;
        let event: CycleProgressEvent = serde_json::from_str(json).unwrap();
        match event {
            CycleProgressEvent::MutationGated { session_id, .. } => {
                assert_eq!(session_id, "");
            }
            _ => panic!("wrong variant"),
        }
    }
}
