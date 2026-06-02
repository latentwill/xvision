# v1 Gaps — Closure Status (as of 2026-06-01)

> **Source spec:** `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`
> **Audit basis:** `git log --since=2026-05-11` + `gh pr list --state merged`

All 8 tracks were closed on **2026-05-11** — the same day the spec was written — through 6 PRs (Tracks B+C were bundled, Track D was a false positive).

---

## Summary table

| Track | Title | Status | PR | Merged |
|---|---|---|---|---|
| A | Findings extraction orchestration | ✅ CLOSED | #62 | 2026-05-11 |
| B | `/eval-runs` row drill-in | ✅ CLOSED | #65 (bundled B+C) | 2026-05-11 |
| C | `/eval-runs` Compare entry point | ✅ CLOSED | #65 (bundled B+C) | 2026-05-11 |
| D | `/eval-runs` error vs empty state | ✅ CLOSED | _no PR_ (false positive) | — |
| E | Inspector "Run eval" CTA | ✅ CLOSED | #68, #70 | 2026-05-11 |
| F | Settings → Danger real implementation | ✅ CLOSED | #67 | 2026-05-11 |
| G | `audit::record` + `health::check` test coverage | ✅ CLOSED | #66 | 2026-05-11 |
| H | Strategies disabled-button affordance | ✅ CLOSED | #69 | 2026-05-11 |

---

## Track A — Findings extraction orchestration

**Status: CLOSED** — PR [#62](https://github.com/latentwill/xvision/pull/62) merged 2026-05-11

**What was done:**
- Created `crates/xvision-engine/src/eval/postprocess.rs` — `extract_and_record` entry point
- Wired into `crates/xvision-engine/src/api/eval.rs` (`run_inner`) at lines 2796 and 3725 (backtest + live/paper paths), not in the executor directly
- Design note in postprocess.rs header explains why: keeping `Executor::run` free of `&ApiContext` was the right seam; orchestration belongs in the API layer

**Key files changed:**
- `crates/xvision-engine/src/eval/postprocess.rs` (created)
- `crates/xvision-engine/src/api/eval.rs` (wired)
- `crates/xvision-engine/src/eval/mod.rs` (module registered)

**Subsequent evolution:**
- PR [#569](https://github.com/latentwill/xvision/pull/569): "feat(eval): memory-aware findings — flag stale recalls driving bad outcomes" — extended the findings extractor with memory-awareness

---

## Track B — `/eval-runs` row drill-in

**Status: CLOSED** — PR [#65](https://github.com/latentwill/xvision/pull/65) merged 2026-05-11 (bundled with Track C)

**What was done:**
- Rows in the eval runs list navigate to `/eval-runs/:runId` on click
- Whole-row click, keyboard support (Tab to focus, Enter to follow), `cursor-pointer` on hover
- The detail route `/eval-runs/:runId` shipped in PR #49 but was previously unreachable from the list

**Key files changed:**
- `frontend/web/src/routes/eval-runs.tsx` (+157 lines total for B+C)

---

## Track C — `/eval-runs` Compare entry point

**Status: CLOSED** — PR [#65](https://github.com/latentwill/xvision/pull/65) merged 2026-05-11 (bundled with Track B)

**What was done:**
- Per-row checkboxes with `selected: Set<string>` state (line 145)
- `CompareToolbar` component appears when ≥1 row selected; disabled until ≥2
- On click: navigates to `/eval-runs/compare?ids=${[...selected].join(",")}`
- Checkbox click uses `stopPropagation` to avoid triggering row navigation

**Key files changed:**
- `frontend/web/src/routes/eval-runs.tsx` (same PR as Track B)

---

## Track D — `/eval-runs` error vs empty state

**Status: CLOSED (false positive)** — No PR needed

The audit flagged this as a gap (checking `data.length === 0` before `isError`). PR #65 investigated and found **the on-disk code already had the correct render order** (`loading → error → empty → table`). No change was required. This was explicitly noted in the PR #65 description:

> "Track D: No change needed — render order is already loading → error → empty → table. The audit flagged this as a gap but the on-disk code already had it right."

---

## Track E — Inspector "Run eval" CTA

**Status: CLOSED** — PR [#68](https://github.com/latentwill/xvision/pull/68) + PR [#70](https://github.com/latentwill/xvision/pull/70) merged 2026-05-11

**What was done:**
- "Run eval →" CTA added to the strategy Inspector page
- Links to `/eval-runs?strategy=${strategyId}&start=1` (pre-selects the strategy)
- PR #68 added the initial CTA; PR #70 added a second placement in the right rail
- Current location: `frontend/web/src/routes/authoring.tsx` line 1403–1406

**Key files changed:**
- `frontend/web/src/routes/authoring.tsx`

---

## Track F — Settings → Danger real implementation

**Status: CLOSED** — PR [#67](https://github.com/latentwill/xvision/pull/67) merged 2026-05-11

**What was done:**
- Created `crates/xvision-engine/src/api/settings/danger.rs` with three audited operations:
  - `reset_workspace` — selective clear of user-authored content (preserves audit, cache, skills)
  - `regen_identity` — overwrite Ed25519 signing key (returns Conflict until wallet plan ships)
  - `factory_reset` — clear all files under `$XVN_HOME`
- Each op requires a distinct typed confirm phrase (not a shared constant)
- Frontend Danger tab replaced the `PlaceholderTab` with real three-section UI

**Subsequent iterations:**
- PR [#237](https://github.com/latentwill/xvision/pull/237): "qa: dashboard auth gate + cli allowlist + danger typed phrases" — hardened auth gate, moved confirm phrases per-op
- PR [#306](https://github.com/latentwill/xvision/pull/306): "round4-selective-reset: replace wipe_db with selective reset_workspace (F-4)" — replaced nuclear `wipe_db` with scoped `reset_workspace` per QA round-4 finding

**Key files changed:**
- `crates/xvision-engine/src/api/settings/danger.rs` (created)
- `crates/xvision-engine/src/api/settings/mod.rs`
- Dashboard danger route
- `frontend/web/src/routes/settings/` (Danger tab)

---

## Track G — `audit::record` + `health::check` test coverage

**Status: CLOSED** — PR [#66](https://github.com/latentwill/xvision/pull/66) merged 2026-05-11

**What was done:**
- The audit's "zero test markers" finding referred to inline `#[cfg(test)]` blocks in the source files. Three audit tests already existed in `tests/api_audit.rs` (integration tests file). PR #66 added only the 2 genuinely missing scenarios + all 4 health tests.

**Added to `crates/xvision-engine/tests/api_audit.rs`:**
- `audit_records_null_target_and_args` — NULL round-trip
- `audit_records_concurrent_writes_yield_distinct_ulids` — 10 concurrent writes, distinct ULIDs

**Added to `crates/xvision-engine/src/api/health.rs` (new `#[cfg(test)] mod tests`):**
- `check_returns_ok_on_fresh_xvn_home` — happy path
- `check_flags_db_when_pool_closed` — DB probe goes Down
- `check_flags_missing_bundles_dir_renders_zero_count_ok` — fresh install sentinel
- `health_report_serialization_round_trip` — wire-shape check

**Note:** `audit.rs` source file has no inline test block; its tests live in `tests/api_audit.rs`. This is intentional — they are integration tests, not unit tests.

---

## Track H — Strategies disabled-button affordance

**Status: CLOSED** — PR [#69](https://github.com/latentwill/xvision/pull/69) merged 2026-05-11

**What was done:**
- Applied `disabled:opacity-50 disabled:cursor-not-allowed` (or equivalent `disabled:opacity-60`) Tailwind utilities to disabled controls in `frontend/web/src/routes/strategies.tsx`
- Subsequent work (PR [#574](https://github.com/latentwill/xvision/pull/574): "Per-strategy DSL Filter MVP") actually implemented the previously-disabled filter functionality, removing the need for disabled-state affordance on those controls entirely

**Key files changed:**
- `frontend/web/src/routes/strategies.tsx`

---

## Post-closure: v1 acceptance criteria

From the spec's cross-cutting acceptance table:

| Criterion | Status |
|---|---|
| 1. Authoring — E adds CTA polish | ✅ CLOSED (Track E + PR #68/#70) |
| 2. Backtest persists metrics + findings | ✅ CLOSED (Track A + PR #62) |
| 3. Alpaca paper + findings | ✅ CLOSED (Track A wired to both backtest + paper paths) |
| 4. Compare side-by-side with findings | ✅ CLOSED (Track A findings + Track C selection UX, PR #65) |
| 5. `xvn eod` markdown report | ✅ Pre-existing (no gap track needed) |
