//! # xvision-dspy — offline prompt/demonstration optimizer
//!
//! This crate is the **offline** optimizer for xvision strategy agents. It wraps
//! the [`dspy-rs`](https://crates.io/crates/dspy-rs) optimizer engine (COPRO /
//! MIPROv2 / GEPA) behind an xvision-facing surface: per-capability *signatures*,
//! a deterministic test-model adapter, and a serializable *optimization snapshot*
//! that records exactly how an instruction/demo set was produced (for lineage and
//! reproducibility).
//!
//! ## OFFLINE-ONLY INVARIANT (hard rule)
//!
//! **This crate exposes NO function that performs a live decision-cycle dispatch.**
//! Optimization happens out-of-band against a *corpus*; the engine's live trading
//! path never calls into here. The only model implementations shipped are:
//!
//! * [`adapter::DeterministicTestModel`] — a `DummyLM`-backed, in-memory,
//!   no-network model used for CI and reproducible optimization runs.
//! * A feature-gated live model stub ([`adapter::live`]) that is *not* wired to
//!   the network in this crate (see its `// TODO live` markers).
//!
//! Correspondingly, `xvision-engine` must NOT depend on `xvision-dspy` or
//! `dspy-rs`. This crate is a graph leaf and is excluded from `default-members`
//! so the heavy transitive tree (rig-core, arrow, parquet, foyer, ...) stays out
//! of the runtime binaries and the slim deploy image.
//!
//! ## Layout
//!
//! * [`capability`] — the small local [`Capability`] enum (Trader, Filter,
//!   DecisionGrader, Intern, ChatAuthoring). Defined here rather than pulled from
//!   `xvision-engine` to preserve the offline isolation invariant.
//! * [`error`] — typed [`OptimizerError`], including `missing_capability_optimizer`
//!   and `provider_unavailable` variants with remediation text.
//! * [`signatures`] — DSRs [`dspy_rs`] signatures per capability, with
//!   parse/validate boundaries.
//! * [`adapter`] — the xvision-facing model trait + the deterministic test model;
//!   provenance (provider/model identity + token/cost accounting).
//! * [`snapshot`] — the serializable [`snapshot::OptimizationSnapshot`] (Phase 3.5
//!   type only; the DB store lands in a later task).

pub mod adapter;
pub mod capability;
pub mod error;
pub mod signatures;
pub mod snapshot;

pub use capability::Capability;
pub use error::{OptimizerError, OptimizerResult};
pub use snapshot::{LineageId, OptimizationSnapshot, SnapshotDemo};
