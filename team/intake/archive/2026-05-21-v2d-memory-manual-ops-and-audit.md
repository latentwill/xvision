# Intake — 2026-05-21 — V2D memory manual operations + audit surface (v1.1)

Follow-up to V2D (merged 2026-05-21 as PR #404). V2D shipped the
memory plumbing — `MemoryMode` toggle, Observations / Patterns cortex
tier split, dispatcher recall + record wiring, MemoryPanel in
eval-review — but with **zero operator surface for managing memory
items**. This intake decomposes the v1.1 wave that adds CLI, API,
and UI for those operations.

## Source

- V2D PR #404 — merged 2026-05-21. Established the storage layer
  (`xvision-memory` crate), the dispatcher recall/record seam, the
  per-slot `memory_mode` toggle, the AgentForm UI selector, the
  MemoryPanel in eval-review, and the operator docs at
  `docs/v2d-memory-overview.md`.
- Design discussion: `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md`
  (F+L+T design, Observations / Patterns terminology, V3
  autooptimizer interplay).
- V2D operator docs themselves call out the gap: the Memory panel
  ships empty in v1 because Patterns is empty, and the docs point
  to a v1.1 manual seeding CLI as the earliest path to populate it.

## Problem statement

V2D as merged is *correct* but *empty*. An operator who flips a
slot's `memory_mode` to `agent_scoped` and runs an eval sees:

- Observations accumulating in the SQLite store (no UI to inspect
  them).
- Patterns staying empty (no UI to seed them, no CLI to add them).
- The Memory panel in eval-review showing "no recall items" forever
  unless V3 autooptimizer ships.

The operator can verify nothing leaked (good), but they can't
*productively use* memory without either waiting for V3 or hand-
editing the SQLite file. That's the v1.1 gap this wave closes.

The wave also makes the leakage-protection design observable: an
operator can see which Patterns the dispatcher considered, which
were excluded by the time-window filter, and what the LLM's
`<prior_observations>` block actually looked like at decision time.
Without that, the F+L+T design lives entirely in tests and trust.

## Current state (what V2D shipped)

- `xvision-memory` crate: `MemoryStore` with `upsert_observation`,
  `upsert_pattern`, `demote_pattern`, `query`, `forget`. SQLite at
  `$XVN_MEMORY_DB` or `~/.xvn/memory.db`.
- `AgentSlot.memory_mode: MemoryMode { Off, Global, AgentScoped }`.
- `MemoryRecorder` wired end-to-end through pipeline + executors.
- `events.jsonl` carries `memory_recall` / `memory_write` /
  `memory_disabled_no_embedder` event kinds.
- AgentForm Memory selector; MemoryPanel in eval-review.
- No CLI verbs for memory.
- No HTTP API endpoints for memory.
- No UI for browsing memory items outside the per-cycle eval-review
  panel.

## Locked decisions

| # | Decision |
|---|---|
| 1 | **Scope = Package B from the post-V2D grill pass.** Read + seed + delete + audit surface; **no manual distillation primitives** (Package C — promote Observation → Pattern). Distillation is V3 autooptimizer's job; doing it manually as a UI feature builds a path that V3 obsoletes. C is **folded into the V3 autooptimizer follow-up** (board-v2 item 11a). |
| 2 | **`xvn memory` CLI surface.** Mirrors the existing `xvn agents` / `xvn strategies` shape. Verbs: `ls`, `show`, `add-pattern`, `rm`, `forget`. No `add-observation` — Observations are engine-written, never operator-written (write-side enforced by the existing `MemoryStore::upsert_observation` provenance assertions). |
| 3 | **HTTP API at `/api/memory`.** Five endpoints: GET list, GET detail, POST patterns, DELETE one, DELETE bulk by namespace/agent. The dashboard's TanStack Query layer consumes this; no separate dashboard-side store. |
| 4 | **Per-agent Memory tab on `/agents/<id>`** is the primary UI placement. A workspace-wide `/memory` route (for the `global` namespace) is a smaller secondary surface. The eval-review MemoryPanel gains a "Open Pattern" link on each recall row that deep-links into the management UI. |
| 5 | **Observations tab is read-only.** Operators cannot add or edit Observations from the UI/CLI. They CAN delete (via `forget --namespace` or `forget --agent`) — bulk only, not per-item — so the operator's only "delete" lever on the Observation tier is namespace-level forget. This keeps Observations honest as a write-once log; per-item observation edits would corrupt the autooptimizer's distillation substrate. |
| 6 | **Patterns are operator-editable.** Operators can add (`add-pattern`), delete (`rm` or `forget`). **Editing a Pattern is deferred to a follow-up** — the design discussion at `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md` notes that the autooptimizer will eventually need "supersede vs replace" semantics; baking edit-in-place now would commit to one shape before V3 spec lands. v1.1 ships add + delete only. |
| 7 | **`training_window_end` is exposed on the CLI but optional in v1.1.** `xvn memory add-pattern` accepts `--training-end <date>`. If omitted, the Pattern is recorded with `training_window_end = NULL` (operator-attested wisdom, recalled in every scenario). The UI form has a "training data ends" date picker, blank by default. |
| 8 | **Delete operations require confirmation in the UI but not the CLI.** UI: a Radix AlertDialog confirms each delete with item count + scope. CLI: `--yes` flag is *not* required (CLI users are assumed to know what they're doing), but `rm` and `forget` print the count of items that will be deleted before the operation completes and emit the count on stdout. |
| 9 | **No new SQLite table.** Everything operates on the existing `memory_items` table from the merged V2D wave. No engine migration. |
| 10 | **API responses include `tier`, full provenance, and `training_window_end` as separate JSON fields.** No mode-specific shape — the same `MemoryItem` JSON ships from both observation-listing and pattern-listing endpoints. The frontend renders different columns per tier. |

## Raw items → tracks

The wave is small. Single-contract decomposition, mirroring V2D's
shape.

| Raw item | Track | Lane | Notes |
|---|---|---|---|
| `xvn memory` CLI verbs (ls / show / add-pattern / rm / forget) | `v2d-memory-cli-and-api` | foundation | CLI lives in `xvision-cli/src/commands/memory.rs`; subcommand structure mirrors `agents.rs`. |
| `/api/memory` HTTP surface (GET list, GET detail, POST patterns, DELETE one, DELETE bulk) | `v2d-memory-cli-and-api` | foundation | Routes added to `xvision-dashboard`; engine functions in `xvision-engine/src/api/memory.rs`. |
| Per-agent Memory tab on `/agents/<id>` | `v2d-memory-cli-and-api` | foundation | New `MemoryTab.tsx` under `frontend/web/src/components/agent/`. Patterns and Observations sub-tabs. |
| Workspace memory page `/memory` | `v2d-memory-cli-and-api` | foundation | Route handler + `frontend/web/src/features/memory/MemoryPage.tsx`. Scoped to `global` namespace. |
| "Open Pattern" deep-link from eval-review MemoryPanel | `v2d-memory-cli-and-api` | foundation | Small change to `MemoryPanel.tsx` — each recall row gets a link to `/agents/<id>?tab=memory&pattern=<id>` (or workspace memory page for global recalls). |

Single contract. Phases inside the contract for review purposes:

```
Phase 1  engine memory module + HTTP routes (CLI + UI both depend on this)
Phase 2  xvn memory CLI verbs
Phase 3  per-agent Memory tab + workspace memory page
Phase 4  eval-review MemoryPanel deep-link wiring
Phase 5  operator docs update (xvn memory subcommand reference,
         in-app wiki Memory subsection extension)
```

## Out of this intake

- **Package C — manual distillation primitives.** Folded into V3
  autooptimizer (`team/board-v2.md` item 11a) — the V3 wave will
  build the same distillation surface as part of the autooptimizer's
  promote/judge/retire loop, so building it twice is wasted effort.
- **Pattern editing.** Decision 6. Defer until V3 autooptimizer
  edit semantics land.
- **Observation per-item delete.** Decision 5. Operators get bulk-
  forget only.
- **Memory export (`xvn memory export --tier observation --out
  memory.jsonl`).** Useful for V3 dev work, not v1 operators. Defer
  until the autooptimizer contract is written.
- **Memory diff (`xvn memory diff --before <date> --after <date>`).**
  Operator audit nicety; not blocking. Defer.
- **Cross-namespace recall blending.** Already deferred in V2D
  intake Decision 4. Same answer here.
- **TTL / decay on Patterns.** Already deferred in V2D intake
  Decision 7. Same answer here.

## Verification (when the track lands)

- Rust unit tests at `crates/xvision-engine/src/api/memory.rs`
  covering the five endpoints (request/response shape, error cases,
  the namespace = `agent:<id>` vs `global` paths).
- CLI integration tests at `crates/xvision-cli/tests/memory_cli.rs`
  covering each verb against an in-memory `MemoryStore`.
- Vitest at `frontend/web/src/features/memory/memory.test.tsx`
  covering the MemoryPage list, the Add Pattern modal, the delete
  confirmation, and the per-agent Memory tab.
- `pnpm --dir frontend/web typecheck` — clean.
- `pnpm --dir frontend/web test --run` — green across the suite.
- `cargo test --workspace` — green (modulo the one pre-existing
  `eval_early_stop` parallel flake from PR #388, not v1.1's concern).
- `bash scripts/board-lint.sh` — green before pushing contract
  edits.
- Manual smoke: open dashboard → `/agents/<id>` → Memory tab →
  see Observations from prior runs → add a Pattern → run an eval
  on a scenario with start date AFTER the Pattern's
  `training_window_end` → confirm the Pattern appears in the
  eval-review MemoryPanel.

## Open questions for the conductor

These resolve at decomposition or contract-claim time:

1. **CLI default tier for `xvn memory ls`?** Patterns is the more
   common operator interest (it's what the agent reads). Recommend
   `--tier pattern` default; switch via `--tier observation`.
2. **API pagination limit?** Patterns is small (operator-curated);
   Observations grows large (every cycle of every run). Recommend
   default `limit=50` on both endpoints; `Observations` endpoint
   accepts `?run_id=<id>` to scope to one run for sensible audit
   queries.
3. **Should `add-pattern` warn when the embedder isn't configured?**
   Without an embedder, the Pattern is stored but never matched by
   cosine recall — silently useless. Recommend: emit a stderr warning
   ("no embedder configured; Pattern will not be recalled until one
   is set up") + a non-zero exit code; the operator can `--force` to
   skip the warning if they're seeding ahead of embedder setup.
4. **UI placement of the workspace `/memory` page in the nav.**
   Settings → Memory? Top-level? Recommend top-level alongside
   `/agents`, `/strategies` — memory is operationally important once
   Patterns is populated.

## Related artifacts

- V2D PR: https://github.com/latentwill/xvision/pull/404
  (merged 2026-05-21, commit `81007d1`)
- V2D plan: `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`
- V2D design note: `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md`
- V2D operator docs: `docs/v2d-memory-overview.md`
- V3 autooptimizer entry (Package C lives here): `team/board-v2.md` item 11a
- AgentForm UI (existing memory selector): `frontend/web/src/components/agent/SlotForm.tsx`
- eval-review MemoryPanel (existing): `frontend/web/src/features/eval-runs/review/MemoryPanel.tsx`
