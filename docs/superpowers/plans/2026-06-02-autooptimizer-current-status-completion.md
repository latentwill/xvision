# AutoOptimizer Current Status Completion Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` before implementing packets from this plan.

**Status date:** 2026-06-02

**Goal:** Finish the non-cryptographic AutoOptimizer operator surface so `/autooptimizer` is useful from real lineage, SSE, ladder, blob, and attribution data. The May 27 crypto provenance spine is historical; do not reintroduce `CycleSeal`, Merkle roots, operator signing, or session commitments.

## Correct Data Model

`GET /api/autooptimizer` is DSPy flywheel history. It is not cycle-level dashboard data.

The `/autooptimizer` dashboard must use:

- `GET /api/autooptimizer/lineage` for kept/rejected experiments, grouped by `cycle_id`
- `GET /api/autooptimizer/events` SSE for live cycle-running signal and event count
- `GET /api/autooptimizer/ladder` for experiment-writer performance
- `GET /api/autooptimizer/blob/:hash` for parent/child bundle diffs
- Cycle summaries derived from lineage nodes, not flywheel run history

## Implemented In Branch

Branch: `task/autooptimizer-ui-align`

Worktree: `/tmp/xvision-autooptimizer-ui-align`

- Frontend normalizes backend SSE envelopes before rendering live events.
- Dashboard summary derives experiment/cycle counts from lineage nodes.
- Genealogy renders per-cycle parent/child trees instead of flat grouped rows.
- Diff inspector fetches child and parent blobs and renders changed JSON paths.
- Provenance groups recent experiments by real lineage writer attribution.
- Backend lineage DTO joins `mutator_attribution` onto `lineage_nodes`.
- Focused frontend tests cover API helpers, live feed, diff, genealogy, and provenance.
- A dashboard route test covers lineage writer attribution.

## Remaining 100x Packets

### Packet A - Rust Verification And Route Fixes

**Goal:** Verify the backend changes compile and the new route test passes on a machine with Rust installed.

Run:

```bash
scripts/cargo test -p xvision-dashboard --test autooptimizer_routes
scripts/cargo check -p xvision-dashboard
```

If failures appear, fix only the backend route/test contract. Do not change frontend behavior unless a backend response shape is corrected.

### Packet B - Real Dashboard Launch Paper Tester

**Goal:** Ensure dashboard-launched evening cycles use the real cached backtest paper-test adapter, not fixed stub metrics.

Files:

- `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`

Expected outcome:

- `start_evening_cycle` builds the same production paper-test path used by the CLI where possible.
- Tests still avoid external broker/network calls.

### Packet C - UX Smoke Pass

**Goal:** Run the Vite dashboard and manually inspect `/autooptimizer`.

Check:

- First screen uses lineage-derived summary cards.
- Live tab does not treat DSPy flywheel history as cycle history.
- Genealogy tree is readable with long hashes.
- Diff table handles root nodes, missing blobs, and unchanged blobs.
- Provenance shows unknown writer attribution only when attribution is genuinely absent.

### Packet D - Full Verification Baseline

**Goal:** Record the branch verification status before PR.

Run:

```bash
pnpm --dir frontend/web test -- --run src/features/autooptimizer/api.test.ts src/features/autooptimizer/LiveCycleView.test.tsx src/features/autooptimizer/DiffInspector.test.tsx src/features/autooptimizer/GenealogyTree.test.tsx src/features/autooptimizer/LadderWithProvenance.test.tsx
pnpm --dir frontend/web typecheck
scripts/cargo test -p xvision-dashboard --test autooptimizer_routes
scripts/cargo check -p xvision-dashboard
```

If the full frontend test suite is run, report existing unrelated failures separately from AutoOptimizer failures.
