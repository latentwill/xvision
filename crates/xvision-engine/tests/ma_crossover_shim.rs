//! Post-2026-05-21: the marketplace baseline `ma_crossover_template()`
//! was removed alongside the strategy `template_registry`. The
//! operator-readable starter content for this baseline migrates to a
//! prepop seed entry surfaced via `xvn strategies init`.
//!
//! The deterministic `Algorithm` implementation used by A/B compare
//! arms (`MaCrossover`) lives separately in
//! `crates/xvision-eval/src/baselines/ma_crossover.rs` and is
//! unaffected by this change.
//!
//! File retained as a historical breadcrumb (see
//! `team/contracts/strategy-template-registry-removal.md`).

#[test]
fn ma_crossover_template_no_longer_exists_documented_marker() {}
