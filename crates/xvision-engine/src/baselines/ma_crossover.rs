//! Marketplace baseline reference for the deterministic MA-crossover
//! strategy.
//!
//! Before the 2026-05-21 template-registry removal this file
//! registered a `ma_crossover_baseline` `Template` into the engine's
//! `template_registry`. With the registry gone, the baseline's
//! operator-readable starter content lives as a markdown seed under
//! `docs/strategies/templates/baselines/ma_crossover_baseline.md` and
//! is surfaced through the strategies folder
//! (`xvn strategies init`).
//!
//! The actual deterministic `Algorithm` implementation that A/B
//! compare arms call lives separately in
//! `crates/xvision-eval/src/baselines/ma_crossover.rs` — that
//! crate's `MaCrossover` struct is unaffected by this change.
//!
//! This module is retained as the seam where, in a follow-up, a
//! per-strategy validator backed by the seed library can land. For
//! now it carries no symbols; the previous `ma_crossover_template()`
//! function and `MaCrossover` `Template` impl have been deleted.
