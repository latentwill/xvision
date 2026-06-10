# Unified Optimizer + Strategy Inspector

**Date:** 2026-06-10  
**Status:** approved  

## Problem

Two separate optimizer code paths exist and diverged:

1. **AutoOptimizer (dashboard/cycle path)** — uses `Mutator::propose()` → `MutationDiff::apply_to()`, sets `AgentRef.prompt_override` on a `Strategy`, runs real paper-test cycles. Lives under `xvn autooptimizer` CLI and `POST /api/autooptimizer/*`.

2. **CLI `xvn optimize run`** — stub: fake deterministic string concatenation for candidates, fake `deterministic_score()` for evaluation, result stored in `optimization_snapshots` and never applied to the agent. Works at the `--agent`/`--slot` level, not the strategy level.

Additionally, optimizer-generated candidate strategies have no UI inspector and cannot be promoted to named strategies for eval use.

## Goals

- One optimizer code path. Both CLI and dashboard call `run_cycle()` → `Mutator::propose()`.
- Full online evaluation — no stubs, no deterministic fake scores, no corpus-based shortcuts.
- `xvn optimize` is the canonical optimizer CLI. `xvn autooptimizer` deprecated.
- Optimizer always creates new immutable strategies (stored in blob store with lineage) — never mutates existing ones.
- Strategy Inspector at `/optimizer/strategy/:hash` lets operators view candidate strategies and promote them to the strategies folder for eval.

## Out of Scope

- "Promote to live" (live strategies inspector not yet wired up).
- Eval run configuration in the inspector (user navigates to `/strategies` and starts eval from there).
- Corpus-based / offline evaluation mode.

---

## Section 1: CLI Command Surface Migration

### `xvn autooptimizer` → deprecated

All `xvn autooptimizer` sub-commands move to `xvn optimize`. The `autooptimizer.rs` handlers become thin shims that print a deprecation notice to stderr and delegate to the `optimize.rs` implementations.

| Deprecated | Canonical |
|---|---|
| `xvn autooptimizer run` | `xvn optimize run` |
| `xvn autooptimizer run-cycle` | `xvn optimize run-cycle` |
| `xvn autooptimizer mutate-once` | `xvn optimize mutate-once` |
| `xvn autooptimizer status` | `xvn optimize status` |
| `xvn autooptimizer pause` | `xvn optimize pause` |
| `xvn autooptimizer resume` | `xvn optimize resume` |
| `xvn autooptimizer list` | `xvn optimize list` |
| `xvn autooptimizer experiment` | `xvn optimize experiment` |

### `xvn optimize` sub-command changes

**Removed (deprecated agent-level commands):**
- `memory-demos`
- `memory-demos-gate`
- `accept-as-child-agent`
- `revert-accepted`
- `export-demos`
- `import-demos`
- `explain-missing-data`

These are handled internally by the optimizer and experiment writer (Mutator). Callers that used them must migrate to the autooptimizer session flow.

**Kept unchanged:**
- `inspect` — read-only diagnostics on a persisted optimization run.

**New canonical `xvn optimize run`:**

```
xvn optimize run --strategy <id> [--cycles <n>] [--mock]
```

- `--strategy <id>` — ULID of the strategy to optimize. Replaces `--agent`/`--slot`/`--capability`/`--corpus`/`--optimizer`/`--metric`/`--max-rounds`/`--rng-seed` (all removed).
- `--cycles <n>` — number of optimization cycles to run (default: 1). Carried over from `xvn autooptimizer run`.
- `--mock` — use `StubPaperTester` for CI / offline testing.

### Implementation approach

`run_optimize()` in `crates/xvision-cli/src/commands/optimize.rs` is rewritten:

1. Resolve strategy from the engine store by `--strategy` id.
2. Construct `Mutator { provider, model, dispatch, max_retries }` from the strategy's bound agent model — same construction as `autooptimizer.rs:1309`.
3. Load `AutoOptimizerConfig` from `autooptimizer.toml` if present; otherwise use `AutoOptimizerConfig::default()` with `allowed_mutation_kinds: ["prose"]`.
4. For each requested cycle: call `run_cycle()` from `xvision-engine::autooptimizer::cycle`.
5. Lineage, blob store writes, and paper-test evaluation all happen inside `run_cycle()`. No new persistence code.

**Engine is untouched.** `cycle.rs`, `mutator.rs`, `lineage.rs`, `blob_store.rs` — no changes. The CLI is purely a routing and argument change.

### Documentation update (subagent)

After implementation, a dedicated subagent updates:
- `.claude/skills/xvision-cli/SKILL.md`
- `.claude/skills/xvision-cli-qa/SKILL.md`
- Any `docs/` references to `xvn autooptimizer` or the old `xvn optimize` argument surface

---

## Section 2: Strategy Inspector UI

### Route

`/optimizer/strategy/:hash`

Accessible as a drill-down from the existing `ExperimentDetail` screen — a "View Strategy" link on the strategy hash sigil navigates here.

Breadcrumb: `OPTIMIZER → cycle → experiment → strategy`

No new sidebar nav entry needed.

### Backend: new API endpoints

**`GET /api/optimizer/strategy/:hash`**  
Loads the strategy JSON from the blob store by content hash. Returns the full `Strategy` struct (same shape as `GET /api/strategy/:id`). 404 if hash not in blob store.

**`GET /api/optimizer/strategy/:hash/diff/origin`**  
Walks the lineage chain from `:hash` back to the root node (`parent_hash IS NULL`). Loads both strategy JSONs from blob store. Computes structural diff between origin and current using a new engine helper `strategy_diff(a: &Strategy, b: &Strategy) -> StrategyDiff` (to be added in `crates/xvision-engine/src/autooptimizer/mutator.rs` alongside `MutationDiff`). `StrategyDiff` mirrors `MutationDiff`'s field shape (prose, params, tools, filter) but is derived by field comparison rather than produced by the LLM. Returns:
```json
{
  "origin_hash": "<hex>",
  "diff": { /* StrategyDiff */ }
}
```

**`POST /api/optimizer/strategy/:hash/promote`**  
Saves the strategy JSON from blob store to the strategies folder. Assigns a generated display name: `optimizer-candidate-<hash[:8]>`. Returns `{ "strategy_id": "<new-id>" }`. Idempotent — if a strategy with this hash already exists in the folder, returns the existing id.

### Frontend: `OptimizerStrategyInspector` screen

File: `frontend/web/src/features/autooptimizer/screens/StrategyInspector.tsx`

Layout mirrors the strategy detail page (`/authoring/:id`) — read-only rendering of agents, risk config, filter, mechanical params, manifest. Below the strategy content, two additional modules are appended.

#### Additional Module: Optimizer Lineage

Three panels displayed vertically:

1. **Gate scorecard** — reuses `GateScorecard` + `GateBadge`. Shows: parent hash, gate verdict, Sharpe scores (day window + held-out window), cycle id, created at, lineage status.

2. **Diff from parent** — reuses `ParentDiffPanel`. Shows the `MutationDiff` applied to produce this candidate from its immediate parent. Collapsible, expanded by default.

3. **Diff from originating strategy** — new panel `OriginDiffPanel`. Data from `GET /api/optimizer/strategy/:hash/diff/origin`. Shows: origin hash sigil, then the cumulative `StrategyDiff` fields (prose changes, param changes, tool changes, filter changes). Collapsible, expanded by default.

#### Additional Module: Promote to Eval

A single action row:

- **"Promote to Eval"** button — calls `POST /api/optimizer/strategy/:hash/promote`. Shows inline loading state. On success navigates to `/strategies`. No confirmation dialog (no-popup rule).
- If already promoted (API returns existing id): button label changes to "Already promoted — view in strategies" and navigates directly.

### Route registration

Add to `routes.tsx`:
```tsx
{ path: "optimizer/strategy/:hash", element: <OptimizerStrategyInspector /> }
```

Add lazy import alongside existing optimizer screen imports (lines 61–64).

---

## Acceptance Criteria

### CLI

- [ ] `xvn optimize run --strategy <id>` resolves the strategy, runs `run_cycle()`, persists lineage in blob store — no stubs, no fake scores
- [ ] `xvn autooptimizer run --strategy <id>` prints deprecation notice to stderr, delegates to `xvn optimize run`, produces identical output
- [ ] All removed sub-commands (`memory-demos`, `accept-as-child-agent`, etc.) return a clear deprecation error
- [ ] `xvn optimize inspect` still works unchanged
- [ ] `cargo test --workspace` green
- [ ] Skills and docs updated by subagent

### Strategy Inspector

- [ ] `/optimizer/strategy/:hash` loads and renders strategy content from blob store
- [ ] Gate scorecard shows correct verdict and scores
- [ ] Diff from parent panel renders correctly for a known experiment hash
- [ ] Diff from origin panel renders correctly and identifies the root node
- [ ] "Promote to Eval" saves strategy to folder and navigates to `/strategies`
- [ ] Idempotent promote — second click navigates without creating a duplicate
- [ ] No modals, no popups — inline loading state only
- [ ] `npm test` green
