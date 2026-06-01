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
        CycleStarted { .. } => "Evening run started",
        ParentSelected { .. } => "Parent selected",
        MutationProposed { .. } => "Experiment proposed",
        MutationGated { passed: true, .. } => "Experiment kept",
        MutationGated { passed: false, .. } => "Experiment dropped",
        HonestyCheckRun { .. } => "Honesty check result",
        JudgeFinding { .. } => "Reviewer finished notes",
        CycleSealed { .. } => "Evening summary signed",
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
        MutationGated { passed: true, .. } => "mutation_gated_passed",
        MutationGated { passed: false, .. } => "mutation_gated_dropped",
        HonestyCheckRun { .. } => "honesty_check_run",
        JudgeFinding { .. } => "judge_finding",
        CycleSealed { .. } => "cycle_sealed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cycle_started() -> CycleProgressEvent {
        CycleProgressEvent::CycleStarted {
            cycle_id: "c1".into(),
            parent_count: 3,
        }
    }
    fn parent_selected() -> CycleProgressEvent {
        CycleProgressEvent::ParentSelected {
            cycle_id: "c1".into(),
            parent_hash: "abc".into(),
        }
    }
    fn mutation_proposed() -> CycleProgressEvent {
        CycleProgressEvent::MutationProposed {
            cycle_id: "c1".into(),
            parent_hash: "abc".into(),
        }
    }
    fn mutation_gated_passed() -> CycleProgressEvent {
        CycleProgressEvent::MutationGated {
            cycle_id: "c1".into(),
            child_hash: "def".into(),
            passed: true,
        }
    }
    fn mutation_gated_dropped() -> CycleProgressEvent {
        CycleProgressEvent::MutationGated {
            cycle_id: "c1".into(),
            child_hash: "def".into(),
            passed: false,
        }
    }
    fn honesty_check_run() -> CycleProgressEvent {
        CycleProgressEvent::HonestyCheckRun {
            cycle_id: "c1".into(),
            passed: true,
        }
    }
    fn judge_finding() -> CycleProgressEvent {
        CycleProgressEvent::JudgeFinding {
            cycle_id: "c1".into(),
            child_hash: "def".into(),
            severity: "low".into(),
            code: "J001".into(),
        }
    }
    fn cycle_sealed() -> CycleProgressEvent {
        CycleProgressEvent::CycleSealed {
            cycle_id: "c1".into(),
            merkle_root: "mr".into(),
            node_count: 5,
        }
    }

    #[test]
    fn display_label_covers_all_variants() {
        assert_eq!(display_label(&cycle_started()), "Evening run started");
        assert_eq!(display_label(&parent_selected()), "Parent selected");
        assert_eq!(display_label(&mutation_proposed()), "Experiment proposed");
        assert_eq!(display_label(&mutation_gated_passed()), "Experiment kept");
        assert_eq!(display_label(&mutation_gated_dropped()), "Experiment dropped");
        assert_eq!(display_label(&honesty_check_run()), "Honesty check result");
        assert_eq!(display_label(&judge_finding()), "Reviewer finished notes");
        assert_eq!(display_label(&cycle_sealed()), "Evening summary signed");
    }

    #[test]
    fn event_kind_covers_all_variants() {
        assert_eq!(event_kind(&cycle_started()), "cycle_started");
        assert_eq!(event_kind(&parent_selected()), "parent_selected");
        assert_eq!(event_kind(&mutation_proposed()), "mutation_proposed");
        assert_eq!(event_kind(&mutation_gated_passed()), "mutation_gated_passed");
        assert_eq!(event_kind(&mutation_gated_dropped()), "mutation_gated_dropped");
        assert_eq!(event_kind(&honesty_check_run()), "honesty_check_run");
        assert_eq!(event_kind(&judge_finding()), "judge_finding");
        assert_eq!(event_kind(&cycle_sealed()), "cycle_sealed");
    }
}
