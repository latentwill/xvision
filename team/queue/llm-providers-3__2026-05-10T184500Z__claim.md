---
from: llm-providers-3
to: all
topic: claim
created_at: 2026-05-10T18:45:00Z
ack_required: false
---

# `llm-providers-3` track claimed (Phase 3 Tasks 9–10 — ProviderRegistry skeleton)

Session 3 (continuing the Plan #7 thread — Phases 1 and 2 merged via PRs #14
and #16) takes a focused slice of Phase 3. Worktree
`.worktrees/llm-providers-3`, branch `feature/llm-providers-phase-3-registry`.

## Scope (purely additive — no existing API changes)

- **T9** — `crates/xvision-eval/src/provider_registry.rs` (new): `ProviderRegistry`
  struct with `rows`, `default_intern`, `default_trader`, intern + trader
  caches (`Mutex<HashMap<(provider, model), Arc<dyn Backend>>>`),
  `intern_backend(slot)` resolver with shorthand-fill + `find_provider`
  diagnostic. 2 failure-path tests.
- **T10** — `trader_backend(slot)` resolver + 2 memoization tests proving
  `Arc::ptr_eq` holds across same-slot calls and breaks on different model.
- `crates/xvision-eval/src/lib.rs` — add `pub mod provider_registry;` export.

## Out of scope (deferred to next session — breaking API changes)

- **T11** — Wire `ProviderRegistry` into `run_ab_compare` (changes signature)
- **T12** — Swap CLI `commands/ab_compare.rs` to `run_ab_compare_v2`
- **Phase 4** — `xvn provider` CLI subcommand
- **Phase 5** — UI design lock + migration note

Splitting at the T10/T11 boundary keeps this PR purely additive: the new
`ProviderRegistry` module compiles and tests but isn't wired into anything
yet. T11 in a follow-up PR can update `run_ab_compare` callers atomically.

## Files this track touches (no overlap with active sessions)

- `crates/xvision-eval/src/provider_registry.rs` (new)
- `crates/xvision-eval/src/lib.rs` (one-line `pub mod` addition)
- `team/MANIFEST.md` + status + this queue file

Zero overlap with:
- `eval-3c-attestation` (session 1, just merged via PR #17): touched
  `crates/xvision-engine/src/eval/`, NOT `crates/xvision-eval/`
- `frontend-2-home-and-health` (PR #13 open) and `frontend-2-settings`
  (PR #18 open): both in dashboard / frontend / api/health.rs

## Why this slice

Builds on Phase 2 (PR #16): `SlotRef` + `ArmKind::Trader` slot fields are
now resolvable to backends. The `ProviderRegistry` is the registry-of-record
that Phase 3 Task 11 will plumb into `run_ab_compare` to actually use the
slots. This sub-slice ships the registry alone so its surface can be reviewed
in isolation.
