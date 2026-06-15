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
        /// Blob hash of the candidate strategy produced by the experiment writer.
        /// Empty string when not yet known at the emit site.
        #[serde(default)]
        child_hash: String,
        /// Model identifier of the experiment writer (mutator) that produced this candidate.
        /// Empty string when not available at the emit site.
        #[serde(default)]
        mutator_model: String,
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
    /// Fired when a candidate's eval FAILED (e.g. the trader model returned an
    /// invalid action, or truncated mid-JSON) and the cycle skipped it to
    /// continue. Distinct from `NoCandidate` (the experiment writer produced
    /// nothing): here a candidate existed but its evaluation crashed. Operator
    /// label: "Candidate eval failed". 2026-06-13 trader-failure resilience.
    CandidateError {
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
        /// Delta of the child's day-window Sharpe minus the parent's day-window
        /// Sharpe. `None` when gate scores were not computed (e.g. early rejection).
        #[serde(default)]
        delta_day: Option<f64>,
        /// WS-11b: the persisted eval `Run.id` for this candidate's primary
        /// day-window evaluation. `None` for paths that don't surface a run id
        /// (test stubs, the regime/no-candidate paths) — the frontend renders
        /// the experiment row without a nested eval-run node in that case.
        /// Lets an operator drill cycle → experiment → that candidate's
        /// eval-run trace.
        #[serde(default)]
        eval_run_id: Option<String>,
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
    /// U5: periodic heartbeat re-emitted from the underlying eval executor's
    /// progress stream while a long backtest (e.g. the parent baseline) is in
    /// flight, so the cycle output stream doesn't go silent for 10–20 minutes
    /// between `ParentSelected` and the next phase boundary. Operators were
    /// cancelling cycles that looked hung at this point. Throttled to ~30s wall
    /// clock at the re-emit site. `decisions` is the count of trader decisions
    /// the eval has emitted so far (0 if not known); `elapsed_s` is wall-clock
    /// seconds since the eval started. Operator label: "Eval progress".
    EvalProgress {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        #[serde(default)]
        decisions: usize,
        #[serde(default)]
        elapsed_s: u64,
    },
    /// U5: a bare heartbeat with no decision count — emitted when the eval
    /// executor signals liveness (a tick) but no new decision has landed. Cheap
    /// "still alive" signal so the CLI/dashboard can render a spinner that
    /// advances instead of a frozen screen. Operator label: "Working".
    Heartbeat {
        #[serde(default)]
        session_id: String,
        cycle_id: String,
        #[serde(default)]
        elapsed_s: u64,
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

    /// MutationProposed with child_hash/mutator_model serializes at top level; old JSON
    /// (without those keys) still deserializes via #[serde(default)].
    #[test]
    fn test_mutation_proposed_new_fields_serde() {
        // New fields serialize at the top level (no "payload" envelope).
        let event = CycleProgressEvent::MutationProposed {
            session_id: "s1".into(),
            cycle_id: "c1".into(),
            parent_hash: "p1".into(),
            child_hash: "ch1".into(),
            mutator_model: "claude-3-5-sonnet".into(),
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["type"], "mutation_proposed");
        assert_eq!(v["child_hash"], "ch1");
        assert_eq!(v["mutator_model"], "claude-3-5-sonnet");

        // Old JSON without the new keys still deserializes (backward compat).
        let old_json = r#"{"type":"mutation_proposed","session_id":"s1","cycle_id":"c1","parent_hash":"p1"}"#;
        let parsed: CycleProgressEvent = serde_json::from_str(old_json).unwrap();
        match parsed {
            CycleProgressEvent::MutationProposed {
                child_hash,
                mutator_model,
                ..
            } => {
                assert_eq!(child_hash, "", "child_hash should default to empty string");
                assert_eq!(mutator_model, "", "mutator_model should default to empty string");
            }
            _ => panic!("wrong variant"),
        }
    }

    /// MutationGated with delta_day serializes at top level; old JSON (without
    /// delta_day) still deserializes with None.
    #[test]
    fn test_mutation_gated_delta_day_serde() {
        // With delta_day populated.
        let event = CycleProgressEvent::MutationGated {
            session_id: "s1".into(),
            cycle_id: "c1".into(),
            child_hash: "ch1".into(),
            passed: true,
            outcome: "kept".into(),
            delta_day: Some(0.042),
            eval_run_id: None,
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["type"], "mutation_gated");
        assert!((v["delta_day"].as_f64().unwrap() - 0.042).abs() < 1e-9);

        // Old JSON without delta_day deserializes to None.
        let old_json =
            r#"{"type":"mutation_gated","cycle_id":"c1","child_hash":"abc","passed":true,"outcome":"kept"}"#;
        let parsed: CycleProgressEvent = serde_json::from_str(old_json).unwrap();
        match parsed {
            CycleProgressEvent::MutationGated { delta_day, .. } => {
                assert_eq!(delta_day, None, "delta_day should default to None");
            }
            _ => panic!("wrong variant"),
        }
    }

    /// WS-11b: MutationGated carries the candidate's eval_run_id at the top
    /// level so the frontend can nest a navigable eval-run node under the
    /// experiment. Old JSON (without eval_run_id) still deserializes to None.
    #[test]
    fn test_mutation_gated_eval_run_id_serde() {
        // With eval_run_id populated, it serializes at the top level.
        let event = CycleProgressEvent::MutationGated {
            session_id: "s1".into(),
            cycle_id: "c1".into(),
            child_hash: "ch1".into(),
            passed: true,
            outcome: "kept".into(),
            delta_day: Some(0.042),
            eval_run_id: Some("01EVALRUNULID".into()),
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["type"], "mutation_gated");
        assert_eq!(v["eval_run_id"], "01EVALRUNULID");

        // Old JSON (no eval_run_id) still deserializes to None — back-compat.
        let old_json =
            r#"{"type":"mutation_gated","cycle_id":"c1","child_hash":"abc","passed":true,"outcome":"kept"}"#;
        let parsed: CycleProgressEvent = serde_json::from_str(old_json).unwrap();
        match parsed {
            CycleProgressEvent::MutationGated { eval_run_id, .. } => {
                assert_eq!(eval_run_id, None, "eval_run_id should default to None");
            }
            _ => panic!("wrong variant"),
        }

        // A candidate that produced no run id (e.g. the regime path) serializes
        // eval_run_id as JSON null and round-trips back to None.
        let no_run = CycleProgressEvent::MutationGated {
            session_id: String::new(),
            cycle_id: "c1".into(),
            child_hash: "ch2".into(),
            passed: false,
            outcome: "dropped".into(),
            delta_day: None,
            eval_run_id: None,
        };
        let nv = serde_json::to_value(&no_run).unwrap();
        assert!(nv["eval_run_id"].is_null());
        let reparsed: CycleProgressEvent = serde_json::from_value(nv).unwrap();
        match reparsed {
            CycleProgressEvent::MutationGated { eval_run_id, .. } => {
                assert_eq!(eval_run_id, None);
            }
            _ => panic!("wrong variant"),
        }
    }

    /// U5: EvalProgress serializes as "eval_progress" (snake_case) with the
    /// new fields at the top level; old JSON missing them defaults cleanly.
    #[test]
    fn test_eval_progress_wire_and_backcompat() {
        let event = CycleProgressEvent::EvalProgress {
            session_id: "s1".into(),
            cycle_id: "c1".into(),
            decisions: 42,
            elapsed_s: 45,
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["type"], "eval_progress");
        assert_eq!(v["decisions"], 42);
        assert_eq!(v["elapsed_s"], 45);

        // Old JSON without session_id/decisions/elapsed_s still deserializes.
        let old_json = r#"{"type":"eval_progress","cycle_id":"c1"}"#;
        let parsed: CycleProgressEvent = serde_json::from_str(old_json).unwrap();
        match parsed {
            CycleProgressEvent::EvalProgress {
                session_id,
                cycle_id,
                decisions,
                elapsed_s,
            } => {
                assert_eq!(session_id, "");
                assert_eq!(cycle_id, "c1");
                assert_eq!(decisions, 0);
                assert_eq!(elapsed_s, 0);
            }
            _ => panic!("wrong variant"),
        }
    }

    /// U5: Heartbeat serializes as "heartbeat" and round-trips with defaults.
    #[test]
    fn test_heartbeat_wire_and_backcompat() {
        let event = CycleProgressEvent::Heartbeat {
            session_id: "s1".into(),
            cycle_id: "c1".into(),
            elapsed_s: 60,
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(v["type"], "heartbeat");
        assert_eq!(v["elapsed_s"], 60);

        let old_json = r#"{"type":"heartbeat","cycle_id":"c1"}"#;
        let parsed: CycleProgressEvent = serde_json::from_str(old_json).unwrap();
        match parsed {
            CycleProgressEvent::Heartbeat {
                session_id,
                elapsed_s,
                ..
            } => {
                assert_eq!(session_id, "");
                assert_eq!(elapsed_s, 0);
            }
            _ => panic!("wrong variant"),
        }
    }

    /// CandidateError serializes as "candidate_error" and carries reason field.
    /// 2026-06-13 trader-failure resilience (Task 3.2).
    #[test]
    fn test_candidate_error_wire_and_reason() {
        let event = CycleProgressEvent::CandidateError {
            session_id: "s1".into(),
            cycle_id: "c1".into(),
            parent_hash: "ph1".into(),
            reason: "trader returned invalid action".into(),
        };
        let v = serde_json::to_value(&event).unwrap();
        assert_eq!(
            v["type"].as_str().unwrap(),
            "candidate_error",
            "CandidateError must serialize as 'candidate_error'"
        );
        assert_eq!(v["reason"].as_str().unwrap(), "trader returned invalid action");

        // Old JSON without session_id/reason still deserializes via #[serde(default)].
        let old_json = r#"{"type":"candidate_error","cycle_id":"c1","parent_hash":"ph1"}"#;
        let parsed: CycleProgressEvent = serde_json::from_str(old_json).unwrap();
        match parsed {
            CycleProgressEvent::CandidateError {
                session_id,
                cycle_id,
                parent_hash,
                reason,
            } => {
                assert_eq!(session_id, "");
                assert_eq!(cycle_id, "c1");
                assert_eq!(parent_hash, "ph1");
                assert_eq!(reason, "");
            }
            _ => panic!("wrong variant"),
        }
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
