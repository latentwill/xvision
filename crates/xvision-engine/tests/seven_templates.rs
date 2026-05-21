//! Post-2026-05-21: the strategy `template_registry` was removed
//! (see `team/contracts/strategy-template-registry-removal.md`).
//!
//! This file previously enumerated the eight registered templates and
//! asserted each produced a `validate_strategy`-passing draft. With
//! the registry gone, operator-readable starters live as prepop seeds
//! under `docs/strategies/templates/`; that surface is covered by
//! `tests/strategies_folder_prepop.rs`.
//!
//! The file is retained (rather than deleted) so the historical
//! contract acceptance stays discoverable via `git log -- tests/`.

#[test]
fn template_registry_no_longer_exists_documented_marker() {
    // Compile-time marker: if the registry path is ever resurrected,
    // the import below will fail. (No reachable symbol from
    // xvision-engine for `templates::registry` after this change.)
    //
    // Acceptance assertion happens at the grep guard in CI:
    // `rg --hidden -n 'template_registry' crates/` must return only
    // documentation comments.
}
