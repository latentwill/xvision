//! AutoOptimizer module — implements AR-1's mutator + lineage + numeric
//! gate per
//! `docs/superpowers/plans/2026-05-09-autooptimizer-1-mutator-lineage-gate-seal.md`.
//! (The cryptographic-provenance seal layer was removed 2026-06-01 — see
//! `docs/superpowers/specs/2026-06-01-remove-autooptimizer-crypto-provenance-design.md`.)
//!
//! This is the scaffold landed by AR-1 Task 1. Each submodule is a
//! placeholder filled in by a later AR-1 task (see the plan's task
//! table). Task 1 declares them up front so subsequent task PRs can
//! land in parallel without colliding on this `mod.rs`.
//!
//! Note: the original plan placed `program_view` under `src/bundle/`,
//! but no `bundle` module exists in `xvision-engine` today. The
//! program view is hosted here under `autooptimizer/program_view`
//! instead — it is logically part of the autooptimizer's mutation
//! surface and the rest of the codebase doesn't currently reference a
//! bundle namespace.
//!
//! Existing HTTP-surface autooptimizer entry points live at
//! `src/api/autooptimizer.rs` and are unrelated to this module — that
//! file is the dashboard API; this module is the cryptographic + LLM
//! substrate the API will eventually delegate to.

pub mod blob_store;
pub mod canary;
pub mod config;
pub mod content_hash;
pub mod cycle;
pub mod cycle_export;
pub mod cycle_loosen;
pub mod cycle_runs;
pub mod diversity;
pub mod dspy_bridge;
pub mod dspy_flywheel;
pub mod eval_adapter;
pub mod events_store;
pub mod evidence;
pub mod gate;
pub mod gepa;
pub mod inversion;
pub mod judge;
pub mod lineage;
pub mod local_dispatch;
pub mod metering_dispatch;
pub mod mutator;
pub mod mutator_ladder;
pub mod parent_policy;
pub mod pattern_snapshot;
pub mod preflight;
pub mod preflight_cycle;
pub mod program_view;
pub mod progress;
pub mod random_baseline;
pub mod regime_results;
pub mod run_lock;
pub mod scenario_synthesis;
pub mod scheduler;
pub mod session;
pub mod tournament;
pub mod validator;

pub use blob_store::BlobStore;
pub use canary::{build_sabotaged_strategy, run_honesty_check, validate_prompt_semantics, HonestyCheckResult, SabotageVariant};
pub use config::{
    AutoOptimizerConfig, BaselineUntouchedWindow, DayWindow, LooseningSchedule, MutatorConfig,
    ScenarioWindowPair, TradeDirection,
};
pub use content_hash::{canonical_json, canonicalize_json, hash_bytes, hash_canonical_json, ContentHash};
pub use cycle::{run_cycle, select_scenario_pair, CycleConfig, CycleResult};
pub use cycle_export::{
    assemble_cycle_export, build_cycle_export, load_cycle_events, operator_label,
    render_cycle_export_markdown, render_cycle_report_markdown, CycleExport, ExperimentSummary,
    SCHEMA_VERSION as CYCLE_EXPORT_SCHEMA_VERSION,
};
// [loosening-disabled 2026-06-22]
// pub use cycle_loosen::{effective_min_improvement_for_cycle, EffectiveGateConfig};
pub use cycle_runs::{
    get_cycle_run, list_cycle_runs, CycleNodeDetail, CycleRunDetail, CycleRunSummary, HonestyCheckRecord,
    NodeProvenance,
};
pub use diversity::{compute_diversity_score, diversity_decay_for_cycle, record_embedding};
pub use eval_adapter::{
    BacktestPaperTester, BudgetCappedPaperTester, CachedBacktestPaperTester, PaperTestRunner, StubPaperTester,
};
pub use events_store::{append_event, prune_old_events};
pub use evidence::{
    ensure_evidence_schema, load_findings, load_gate_record, persist_finding, persist_gate_record,
    FindingRow, GateRecord, GateRecordRow,
};
pub use gate::{evaluate, GateInput, GateVerdict};
pub use inversion::{invert_mutation, run_inversion_pair, InversionPairResult};
pub use lineage::{LineageNode, LineageStatus, LineageStore};
pub use local_dispatch::AutoOptimizerLocalDispatch;
pub use metering_dispatch::{CostMeteringDispatch, CycleMeter, MaxTokensCapDispatch};
pub use mutator::{MutationDiff, MutationKind, Mutator, ParamChange, ProseEdit, ToolDiff};
pub use mutator_ladder::{compute_ladder, record_outcome, record_proposal, MutatorScore};
pub use parent_policy::{select_parents, ParentPolicy, ScoreField};
pub use program_view::{from_markdown, round_trip_invariant_ok, to_markdown, ProgramViewError};
pub use scenario_synthesis::synthesize_baseline_untouched_scenario;
pub use session::{
    create_session, create_session_with_id, ensure_session_schema, get_active_session,
    increment_cycle_completed, loosening_floor_reached, mark_interrupted_sessions, run_session,
    transition_state, CycleRunOutcome, OptimizerSession,
};
pub use validator::{validate_mutation_diff, ValidationError};
pub mod anti_pattern;
