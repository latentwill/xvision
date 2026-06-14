//! Operator-facing display labels for autooptimizer SSE events.
//!
//! Maps `CycleProgressEvent` variants (developer-surface wire names) to
//! plain-language strings for operator-facing display. Follows the
//! terminology lock in `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md` §11.
//!
//! # Maintenance note
//! When a new `CycleProgressEvent` variant is added, this match must be
//! updated simultaneously. The `display_label_covers_all_variants` test
//! below provides an exhaustive check. Also update the corresponding JS-side
//! mapping in `crates/xvision-dashboard/static/js/bus.js`.

use xvision_engine::autooptimizer::progress::CycleProgressEvent;

/// Returns the operator-facing display label for a `CycleProgressEvent`.
///
/// The wire name (the `type` serde discriminant on the enum) is the SSE
/// protocol identifier and never changes. The display label is what the
/// dashboard renders to the operator.
pub fn display_label(event: &CycleProgressEvent) -> &'static str {
    use CycleProgressEvent::*;
    match event {
        CycleStarted { .. } => "Optimizer run started",
        ParentSelected { .. } => "Parent selected",
        MutationProposed { .. } => "Experiment proposed",
        NoCandidate { .. } => "No experiment produced",
        CandidateError { .. } => "Candidate eval failed",
        MutationGated { outcome, .. } if outcome == "suspect" => "Experiment suspect",
        MutationGated { passed: true, .. } => "Experiment kept",
        MutationGated { passed: false, .. } => "Experiment dropped",
        HonestyCheckRun { .. } => "Honesty check result",
        JudgeFinding { .. } => "Reviewer finished notes",
        CycleFinished { .. } => "Optimizer run finished",
        PhaseStarted { .. } => "Phase started",
        PhaseFinished { .. } => "Phase finished",
        EvalProgress { .. } => "Backtest progress",
        Heartbeat { .. } => "Working…",
        SessionStateChanged { .. } => "Run state changed",
        FlywheelCompiled { .. } => "Findings compiled into prompt pattern",
    }
}

/// Returns the snake_case wire name for a `CycleProgressEvent`, matching
/// the `serde(rename_all = "snake_case")` discriminant on the enum.
/// Used as both the SSE `event:` frame name and the `kind` field in the
/// JSON payload.
pub fn event_kind(event: &CycleProgressEvent) -> &'static str {
    use CycleProgressEvent::*;
    match event {
        CycleStarted { .. } => "cycle_started",
        ParentSelected { .. } => "parent_selected",
        MutationProposed { .. } => "mutation_proposed",
        NoCandidate { .. } => "no_candidate",
        CandidateError { .. } => "candidate_error",
        MutationGated { outcome, .. } if outcome == "suspect" => "mutation_gated_suspect",
        MutationGated { passed: true, .. } => "mutation_gated_passed",
        MutationGated { passed: false, .. } => "mutation_gated_dropped",
        HonestyCheckRun { .. } => "honesty_check_run",
        JudgeFinding { .. } => "judge_finding",
        CycleFinished { .. } => "cycle_finished",
        PhaseStarted { .. } => "phase_started",
        PhaseFinished { .. } => "phase_finished",
        EvalProgress { .. } => "eval_progress",
        Heartbeat { .. } => "heartbeat",
        SessionStateChanged { .. } => "session_state_changed",
        FlywheelCompiled { .. } => "flywheel_compiled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_engine::autooptimizer::progress::Phase;

    fn cycle_started() -> CycleProgressEvent {
        CycleProgressEvent::CycleStarted {
            session_id: "".into(),
            cycle_id: "c1".into(),
            parent_count: 3,
        }
    }
    fn parent_selected() -> CycleProgressEvent {
        CycleProgressEvent::ParentSelected {
            session_id: "".into(),
            cycle_id: "c1".into(),
            parent_hash: "abc".into(),
        }
    }
    fn mutation_proposed() -> CycleProgressEvent {
        CycleProgressEvent::MutationProposed {
            session_id: "".into(),
            cycle_id: "c1".into(),
            parent_hash: "abc".into(),
            child_hash: "".into(),
            mutator_model: "".into(),
        }
    }
    fn mutation_gated_passed() -> CycleProgressEvent {
        CycleProgressEvent::MutationGated {
            session_id: "".into(),
            cycle_id: "c1".into(),
            child_hash: "def".into(),
            passed: true,
            outcome: "kept".into(),
            delta_day: None,
        }
    }
    fn mutation_gated_suspect() -> CycleProgressEvent {
        CycleProgressEvent::MutationGated {
            session_id: "".into(),
            cycle_id: "c1".into(),
            child_hash: "def".into(),
            passed: false,
            outcome: "suspect".into(),
            delta_day: None,
        }
    }
    fn mutation_gated_dropped() -> CycleProgressEvent {
        CycleProgressEvent::MutationGated {
            session_id: "".into(),
            cycle_id: "c1".into(),
            child_hash: "def".into(),
            passed: false,
            outcome: "dropped".into(),
            delta_day: None,
        }
    }
    fn honesty_check_run() -> CycleProgressEvent {
        CycleProgressEvent::HonestyCheckRun {
            session_id: "".into(),
            cycle_id: "c1".into(),
            passed: true,
            sabotage_variant: "kill-trades".into(),
            message: "Honesty check passed: sabotaged variant `kill-trades` was correctly rejected.".into(),
        }
    }
    fn judge_finding() -> CycleProgressEvent {
        CycleProgressEvent::JudgeFinding {
            session_id: "".into(),
            cycle_id: "c1".into(),
            child_hash: "def".into(),
            severity: "low".into(),
            code: "J001".into(),
        }
    }
    fn no_candidate() -> CycleProgressEvent {
        CycleProgressEvent::NoCandidate {
            session_id: "".into(),
            cycle_id: "c1".into(),
            parent_hash: "abc".into(),
            reason: "all proposals were no-ops".into(),
        }
    }
    fn cycle_finished() -> CycleProgressEvent {
        CycleProgressEvent::CycleFinished {
            session_id: "".into(),
            cycle_id: "c1".into(),
            active_count: 1,
            suspect_count: 1,
            rejected_count: 1,
        }
    }
    fn phase_started() -> CycleProgressEvent {
        CycleProgressEvent::PhaseStarted {
            session_id: "".into(),
            cycle_id: "c1".into(),
            parent_hash: Some("abc".into()),
            phase: Phase::WriterProposing,
            detail: "".into(),
        }
    }
    fn phase_finished() -> CycleProgressEvent {
        CycleProgressEvent::PhaseFinished {
            session_id: "".into(),
            cycle_id: "c1".into(),
            parent_hash: Some("abc".into()),
            phase: Phase::GateEvaluating,
            duration_ms: 42,
        }
    }
    fn session_state_changed() -> CycleProgressEvent {
        CycleProgressEvent::SessionStateChanged {
            session_id: "s1".into(),
            state: "running".into(),
        }
    }
    fn flywheel_compiled() -> CycleProgressEvent {
        CycleProgressEvent::FlywheelCompiled {
            session_id: "s1".into(),
            cycle_id: "c1".into(),
            optimization_run_id: "r1".into(),
            pattern_id: "p1".into(),
        }
    }
    fn eval_progress() -> CycleProgressEvent {
        CycleProgressEvent::EvalProgress {
            session_id: "".into(),
            cycle_id: "c1".into(),
            decisions: 42,
            elapsed_s: 45,
        }
    }
    fn heartbeat() -> CycleProgressEvent {
        CycleProgressEvent::Heartbeat {
            session_id: "".into(),
            cycle_id: "c1".into(),
            elapsed_s: 60,
        }
    }
    #[test]
    fn display_label_covers_all_variants() {
        assert_eq!(display_label(&cycle_started()), "Optimizer run started");
        assert_eq!(display_label(&parent_selected()), "Parent selected");
        assert_eq!(display_label(&mutation_proposed()), "Experiment proposed");
        assert_eq!(display_label(&mutation_gated_passed()), "Experiment kept");
        assert_eq!(display_label(&mutation_gated_suspect()), "Experiment suspect");
        assert_eq!(display_label(&mutation_gated_dropped()), "Experiment dropped");
        assert_eq!(display_label(&honesty_check_run()), "Honesty check result");
        assert_eq!(display_label(&judge_finding()), "Reviewer finished notes");
        assert_eq!(display_label(&no_candidate()), "No experiment produced");
        assert_eq!(display_label(&cycle_finished()), "Optimizer run finished");
        assert_eq!(display_label(&phase_started()), "Phase started");
        assert_eq!(display_label(&phase_finished()), "Phase finished");
        assert_eq!(display_label(&eval_progress()), "Backtest progress");
        assert_eq!(display_label(&heartbeat()), "Working…");
        assert_eq!(display_label(&session_state_changed()), "Run state changed");
        assert_eq!(
            display_label(&flywheel_compiled()),
            "Findings compiled into prompt pattern"
        );
    }

    #[test]
    fn event_kind_covers_all_variants() {
        assert_eq!(event_kind(&cycle_started()), "cycle_started");
        assert_eq!(event_kind(&parent_selected()), "parent_selected");
        assert_eq!(event_kind(&mutation_proposed()), "mutation_proposed");
        assert_eq!(event_kind(&mutation_gated_passed()), "mutation_gated_passed");
        assert_eq!(event_kind(&mutation_gated_suspect()), "mutation_gated_suspect");
        assert_eq!(event_kind(&mutation_gated_dropped()), "mutation_gated_dropped");
        assert_eq!(event_kind(&honesty_check_run()), "honesty_check_run");
        assert_eq!(event_kind(&judge_finding()), "judge_finding");
        assert_eq!(event_kind(&no_candidate()), "no_candidate");
        assert_eq!(event_kind(&cycle_finished()), "cycle_finished");
        assert_eq!(event_kind(&phase_started()), "phase_started");
        assert_eq!(event_kind(&phase_finished()), "phase_finished");
        assert_eq!(event_kind(&eval_progress()), "eval_progress");
        assert_eq!(event_kind(&heartbeat()), "heartbeat");
        assert_eq!(event_kind(&session_state_changed()), "session_state_changed");
        assert_eq!(event_kind(&flywheel_compiled()), "flywheel_compiled");
    }
}
