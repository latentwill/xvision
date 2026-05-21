//! Post-2026-05-21: the strategy `template_registry` was removed.
//!
//! Previous tests exercised `registry::get("mean_reversion")` and
//! asserted the draft validated. With the registry gone, equivalent
//! coverage moves to `tests/strategies_folder_prepop.rs` (the
//! prepop-seed pipeline that backs the post-removal starter library).
//!
//! File retained as a historical breadcrumb (see
//! `team/contracts/strategy-template-registry-removal.md`).

#[test]
fn template_registry_no_longer_exists_documented_marker() {}
