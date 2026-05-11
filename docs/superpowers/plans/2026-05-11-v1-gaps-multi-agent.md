# v1 Gaps — Multi-Agent Implementation Spec

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement one track at a time. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Source:** Cross-cutting v1 gap audit on 2026-05-11 (4 parallel Explore agents over the post-merge state of `main` after PRs #54/#55/#56). Findings reproduced in this file's "Why each track exists" section.
> **Coordination:** Each track is a separate worktree + branch + PR. Claim by writing `team/queue/<track>__<utc>__claim.md` and updating `team/MANIFEST.md`. The conflicts table below shows which tracks touch the same files; coordinator should sequence those, not parallelize them.

---

**Goal:** Close the gaps that block v1 acceptance per `v1-shipping-plan.md` §168 ("Telemetry, success criteria, exit checks") so a fresh user can complete the five demo flows end-to-end without hitting a placeholder, dead link, or empty findings panel.

**Scope discipline:** This spec covers gaps in the *already-shipped* v1 surface. It does NOT pull deferred features (live daemon, marketplace, wallet plan, journal) into v1. If a track turns out to need a deferred feature, downscope to the v1-only path and leave a FOLLOWUPS line.

**Out of scope:**
- Anything from the deferred archetypes roadmap.
- Visual redesign / theme work.
- Tests for crates outside the audit's hot list (this spec only adds tests where audit found a critical-path coverage hole).
- Stale worktree cleanup (housekeeping the operator can do directly).

---

## Track manifest

| Track | Title | Severity | Files (primary) | Conflicts with | Est. |
|---|---|---|---|---|---|
| A | Findings extraction orchestration | 🔴 BLOCKER | `crates/xvision-engine/src/eval/executor/{backtest,paper}.rs` (+ new `eval/postprocess.rs`) | none | 1 day |
| B | `/eval-runs` row drill-in | 🔴 BLOCKER | `frontend/web/src/routes/eval-runs.tsx` | C, D | 0.5 day |
| C | `/eval-runs` Compare entry point | 🔴 BLOCKER | `frontend/web/src/routes/eval-runs.tsx` | B, D | 0.5 day |
| D | `/eval-runs` error vs empty state | 🟡 GAP | `frontend/web/src/routes/eval-runs.tsx` | B, C | 0.25 day |
| E | Inspector "Run eval" CTA | 🟡 GAP | `frontend/web/src/routes/authoring.tsx` (+ small route helper) | none | 0.5 day |
| F | Settings → Danger real implementation | 🟡 GAP | `frontend/web/src/routes/settings/index.tsx` (+ engine API, dashboard route) | none | 1 day |
| G | `audit::record` + `health::check` test coverage | 🟡 GAP | `crates/xvision-engine/src/api/{audit,health}.rs` | none | 0.5 day |
| H | Strategies disabled-button affordance | 🟢 NIT | `frontend/web/src/routes/strategies.tsx` | none | 0.25 day |

**Critical-path order:** A (blocker, no deps) ∥ B (blocker, fast) ∥ G (test coverage, fast). C and D both modify `eval-runs.tsx`; bundle them with B into a single track if the same agent picks all three (recommended). E, F, H are independent.

---

## Track A — Findings extraction orchestration

**Why this track exists:** The audit found that `extract_findings()` in `crates/xvision-engine/src/eval/findings/extractor.rs:31` is defined but no executor calls it. `eval/store.rs:381` defines `record_finding()` but is only called from tests. Result: every backtest and paper run completes with metrics + equity but **zero findings** — the Compare view's findings column is always empty, the Run Detail page's findings list is always empty, and v1 success criterion #2 ("persists metrics + findings") fails.

**Goal:** Findings get extracted and persisted as part of the run finalization path. Best-effort: extraction failure must NOT fail the run (metrics are the load-bearing artifact).

**Files:**
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs` — call postprocess after `store.finalize`
- Modify: `crates/xvision-engine/src/eval/executor/paper.rs` — same
- Create: `crates/xvision-engine/src/eval/postprocess.rs` — single entry point both executors call
- Modify: `crates/xvision-engine/src/eval/mod.rs` — register module
- Tests: `crates/xvision-engine/tests/eval_findings.rs` — extend existing tests with an executor-driven case

**Steps:**

- [ ] **A.1: Design `postprocess::extract_and_record`**

  Pure async fn taking `(ctx: &ApiContext, run_id: &str)` that:
  1. Loads the run + decisions + equity from `RunStore`
  2. Resolves the LLM dispatch (same env-bound `AnthropicDispatch::new(ANTHROPIC_API_KEY)` pattern as `api::eval::run`)
  3. Calls `extract_findings(...)`
  4. For each finding, calls `store.record_finding(...)` and `api_search::upsert_finding(ctx, &f)`
  5. Returns `Ok(n_recorded)` on success; logs at `warn!` and returns `Ok(0)` on any error (best-effort)

  Audit-log the operation as `("eval", "extract_findings", Some(run_id), …)` so the eod report sees it.

- [ ] **A.2: Wire into `BacktestExecutor::run`**

  After `store.finalize` returns Ok and BEFORE the executor's outer `Ok` return, call `postprocess::extract_and_record(ctx, &run.id).await`. Discard the count; rely on the audit log. Pass `ctx` through — currently the executor takes `&store`, not `&ApiContext`. Either thread `ctx` down or accept that the executor builds a partial context from the store's pool. The cleanest seam: change `Executor::run` signature to take `&ApiContext` instead of `&RunStore` (the store is `RunStore::new(ctx.db.clone())` anyway). If that ripple is too big, extract the dispatch construction into the postprocess fn and pass only the pool — but then audit logging is awkward. Do the signature change.

- [ ] **A.3: Wire into `PaperExecutor::run`**

  Identical to A.2.

- [ ] **A.4: Tests**

  - Unit test on `postprocess::extract_and_record` with a `MockDispatch` that returns a hardcoded `[Finding]` JSON, asserting `eval_findings` rows appear after the call.
  - Integration test that runs the full `BacktestExecutor::run` with `MockDispatch` for both the trader slot AND the findings extractor, then queries `read_findings(run_id)` and asserts non-empty.
  - Regression test: extractor that returns malformed JSON → run still completes successfully with 0 findings + a warn log.

- [ ] **A.5: Verify acceptance criterion #2 end-to-end**

  ```sh
  XVN_HOME=$(mktemp -d) cargo run -p xvision-cli -- eval run \
    --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
  cargo run -p xvision-cli -- eval show <run_id>   # findings section non-empty
  ```

- [ ] **A.6: Commit + PR**

  ```bash
  git commit -m "feat(eval): orchestrate findings extraction in executor finalize path (v1 gap A)"
  ```

**Acceptance:**
- A backtest run against any seeded strategy produces ≥1 row in `eval_findings`.
- The Compare view (`/eval-runs/compare?ids=<a>,<b>`) renders findings.
- Extractor LLM error doesn't cause the run to fail.

**Out of scope for this track:** prompt-tuning the extractor, changing the `Finding` schema, body-text indexing in search.

---

## Track B — `/eval-runs` row drill-in

**Why this track exists:** Audit found `frontend/web/src/routes/eval-runs.tsx:67-95` renders `<tr>` rows with hover styling but zero `Link`/`onClick`/`navigate`. The detail route `/eval-runs/:runId` exists and works (shipped via PR #49 / Plan 2D) but is unreachable from the list.

**Goal:** Clicking any row in the runs table navigates to the run detail page.

**Files:**
- Modify: `frontend/web/src/routes/eval-runs.tsx` — make rows clickable

**Steps:**

- [ ] **B.1: Wrap row navigation**

  Inside the `items.map` (line 67), make the entire row navigate. Two reasonable approaches:
  - Wrap the first `<td>` content in `<Link to={`/eval-runs/${row.id}`}>` and keep the rest of the cells static. Tab order stays sane.
  - Or add `role="link"`, `tabIndex={0}`, `onClick={() => navigate(`/eval-runs/${row.id}`)}`, `onKeyDown` for Enter, and `cursor-pointer` to the `<tr>`.

  Prefer the second (whole-row click) for usability; the existing hover style already implies the row is interactive.

- [ ] **B.2: Test**

  Manual: load `/eval-runs` (with at least one seeded run), click a row, confirm `/eval-runs/<id>` loads with the run detail.

- [ ] **B.3: Commit + PR**

  ```bash
  git commit -m "feat(frontend): /eval-runs rows navigate to detail (v1 gap B)"
  ```

**Acceptance:**
- Clicking a row navigates to `/eval-runs/<id>`.
- Keyboard navigation works (Tab to row, Enter to follow).
- Cursor changes to pointer on hover.

**Conflicts:** C and D both modify the same file. Bundle into one PR if the same agent picks them.

---

## Track C — `/eval-runs` Compare entry point

**Why this track exists:** `/eval-runs/compare?ids=a,b` was wired in PR #50/#52 (acceptance criterion #4) but `/eval-runs` has no UI to assemble the `ids=` query. Users would have to URL-hack to reach it.

**Goal:** Select ≥2 runs in the list, click "Compare", land on the comparison view.

**Files:**
- Modify: `frontend/web/src/routes/eval-runs.tsx`

**Steps:**

- [ ] **C.1: Add a per-row checkbox + selected-set state**

  `useState<Set<string>>(new Set())` for selected run ids. Each row gets a leading `<td>` with a checkbox bound to `selected.has(row.id)`. Toggle on change. Stop propagation so the checkbox doesn't trigger Track B's row-navigation.

- [ ] **C.2: "Compare (n)" button**

  Sticky button above or beside the table that's disabled when `selected.size < 2` and labeled `Compare (${selected.size})`. On click: `navigate(`/eval-runs/compare?ids=${[...selected].join(",")}`)`.

- [ ] **C.3: Test**

  Manual: load `/eval-runs` with ≥2 runs, select two rows, click Compare, confirm the compare view loads with both runs.

- [ ] **C.4: Commit + PR**

  ```bash
  git commit -m "feat(frontend): /eval-runs Compare selection + navigation (v1 gap C)"
  ```

**Acceptance:**
- Selecting <2 runs disables the Compare button.
- Selecting ≥2 runs and clicking Compare loads `/eval-runs/compare?ids=...` with the right ids.
- Checkbox click does NOT trigger row navigation (stop propagation).

**Conflicts:** B, D — same file.

---

## Track D — `/eval-runs` error vs empty state

**Why this track exists:** Audit found the route checks `q.data && q.data.length === 0` before `isError`, so a network failure renders "no runs yet" instead of an error message.

**Goal:** Network failures show a real error state (with retry); empty list shows the "no runs yet" copy.

**Files:**
- Modify: `frontend/web/src/routes/eval-runs.tsx`

**Steps:**

- [ ] **D.1: Reorder render conditions**

  Pattern:
  ```tsx
  if (q.isPending) return <LoadingSkeleton />;
  if (q.isError) return <ErrorState onRetry={() => q.refetch()} message={q.error.message} />;
  if (q.data.length === 0) return <EmptyState />;
  return <RunsTable items={q.data} />;
  ```
  Reuse the existing `ErrorState` styling from `routes/eval-runs-detail.tsx` if present, or copy the chat rail's error-pill styling.

- [ ] **D.2: Test**

  Manual: stop the dashboard backend mid-load; refresh `/eval-runs`; confirm an error state appears (not "no runs").

- [ ] **D.3: Commit + PR**

**Acceptance:**
- 200 + empty payload → "no runs yet" copy.
- 5xx / network error → error state with retry button.

**Conflicts:** B, C — same file.

---

## Track E — Inspector "Run eval" CTA

**Why this track exists:** After editing a draft in the Inspector (`/authoring/<id>`) the user has no in-page CTA to launch an eval of that bundle. They have to navigate to `/eval-runs` and pick the strategy from a dropdown there. Workflow gap, not a blocker.

**Goal:** A "Run eval" button on the Inspector that pre-selects the current strategy in the eval flow.

**Files:**
- Modify: `frontend/web/src/routes/authoring.tsx` — add CTA button in the page header
- Possibly modify: `frontend/web/src/routes/eval-runs.tsx` (or a future `/eval-runs/new` route) to honor `?strategy=<id>` query param as a pre-selection

**Steps:**

- [ ] **E.1: Add CTA**

  In the Inspector page header (next to the "Validate" or "Save" buttons): a "Run eval →" link that routes to `/eval-runs?strategy=<id>` (or whatever the eval-create route is — verify what `/eval-runs?new=1` maps to today).

- [ ] **E.2: Wire pre-selection**

  If `/eval-runs` renders a "new run" form with a strategy dropdown, parse `?strategy=<id>` from the URL and pre-select it. If there's no such form yet, create a minimal one OR drop this step and just navigate to the list (the CTA still has value as a workflow hint).

- [ ] **E.3: Test**

  Manual: open an Inspector page, click Run eval, confirm the strategy is pre-selected (or the user lands somewhere that says "to run this strategy, …").

- [ ] **E.4: Commit + PR**

**Acceptance:**
- Inspector has a discoverable CTA to launch an eval.
- The clicked CTA pre-fills the strategy id (or, if no new-run form exists yet, lands on the runs list with a banner guiding the user).

**Conflicts:** Mild risk of touching `routes/eval-runs.tsx` if a new-run form is added there; coordinate with B/C/D if simultaneous.

---

## Track F — Settings → Danger real implementation

**Why this track exists:** Audit found `frontend/web/src/routes/settings/index.tsx:115-122` returns a "coming soon" placeholder. v1 spec lists `/settings/danger` as in-scope.

**Goal:** A real Danger zone with at least: "wipe local DB" (with confirmation), "regenerate signing key" (with warning), and "factory reset" (rm -rf XVN_HOME, with double confirmation). All ops dispatch through `engine::api::settings::danger::*` so the audit log captures them.

**Files:**
- Create: `crates/xvision-engine/src/api/settings/danger.rs` — wipe / reset / regen helpers
- Modify: `crates/xvision-engine/src/api/settings/mod.rs` — `pub mod danger;`
- Create: `crates/xvision-dashboard/src/routes/settings/danger.rs` — `POST /api/settings/danger/{wipe-db,regen-identity,factory-reset}`
- Modify: `crates/xvision-dashboard/src/server.rs` — wire routes
- Modify: `frontend/web/src/api/settings.ts` — add danger ops
- Modify: `frontend/web/src/routes/settings/index.tsx` — replace `PlaceholderTab` with a real `SettingsDangerRoute`

**Steps:**

- [ ] **F.1: Engine API**

  Three audited fns in `engine::api::settings::danger`:
  - `wipe_db(ctx)` — `DELETE FROM` every table in `xvn.db` *except* `api_audit` (preserve the trail of the wipe itself). Returns row counts.
  - `regen_identity(ctx)` — overwrite `~/.xvn/identity/signing.key` with a new Ed25519 key. Returns the new pubkey.
  - `factory_reset(ctx)` — `tokio::fs::remove_dir_all(&ctx.xvn_home)` then `create_dir_all`. Audit row written FIRST so it survives the wipe (or written separately to a log file, since the DB is about to be gone). Document the trade-off.

- [ ] **F.2: Dashboard routes**

  Three POST routes mirroring F.1. All require a `confirm: "yes-i-am-sure"` field in the request body. 400 if the confirm string doesn't match.

- [ ] **F.3: Frontend Danger page**

  Three sections, each with a clear warning + two-step confirm (modal or inline "type DELETE to confirm" pattern). Use the design system's `danger` color tokens. Per workspace CLAUDE.md: low-opacity dark-variant backgrounds for any colored elements.

- [ ] **F.4: Tests**

  - `engine::api::settings::danger::tests` — happy path for each fn, confirm-string mismatch path, audit-log persistence.
  - `crates/xvision-dashboard/tests/danger_routes.rs` — POST without confirm → 400, with confirm → 200 + side effect.

- [ ] **F.5: Commit + PR**

**Acceptance:**
- Each danger op records to `api_audit` (or a fallback log file for `factory_reset`).
- The UI requires explicit confirmation before sending the POST.
- A wiped DB still has the audit row that recorded the wipe.

**Out of scope:** undoing a danger op, soft-delete, snapshot-before-wipe.

---

## Track G — `audit::record` + `health::check` test coverage

**Why this track exists:** Audit found these two critical-path modules in `crates/xvision-engine/src/api/` have zero test markers. `audit::record` is on every API call's success and error path; `health::check` gates dashboard startup. Both are load-bearing and untested.

**Goal:** Direct unit tests for both, covering the success paths and the error paths the rest of the codebase relies on.

**Files:**
- Modify: `crates/xvision-engine/src/api/audit.rs` — add `#[cfg(test)] mod tests`
- Modify: `crates/xvision-engine/src/api/health.rs` — same

**Steps:**

- [ ] **G.1: `audit::record` tests**

  - `record_inserts_one_row` — call once, query `api_audit` for the row count + every column.
  - `record_with_error_outcome_persists_error_message` — pass `Outcome::Error("boom".into())`, assert `error` column == "boom" and `outcome` == "error".
  - `record_handles_missing_target_and_args` — pass `None` for both, confirm row inserts cleanly with NULL columns.
  - `record_concurrent_writes` — spawn 10 concurrent calls against the same pool, assert 10 distinct ULIDs land.

- [ ] **G.2: `health::check` tests**

  - `check_returns_ok_on_fresh_xvn_home` — happy path against a tempdir.
  - `check_flags_missing_db` — point at a path where `xvn.db` can't be opened, assert the `db` probe is not "ok".
  - `check_flags_missing_bundles_dir` — confirm the bundles probe surfaces "0 (no bundles dir yet)" or equivalent without erroring.
  - `check_serialization_round_trip` — `serde_json::to_string` + `from_str` produce the same `HealthReport`.

- [ ] **G.3: Commit + PR**

**Acceptance:**
- `cargo test -p xvision-engine api::audit` and `api::health` both green.
- New tests don't increase total runtime by >100ms.

**Out of scope:** end-to-end audit-log read tests (those belong with whichever read API is added later).

---

## Track H — Strategies disabled-button affordance

**Why this track exists:** Audit found `frontend/web/src/routes/strategies.tsx:76-89` has filter inputs and buttons with `disabled` + `title="Coming in Plan 3/4"` but no visual disabled styling. Users may click and get no feedback.

**Goal:** Disabled controls look disabled (lower opacity, `cursor-not-allowed`).

**Files:**
- Modify: `frontend/web/src/routes/strategies.tsx`

**Steps:**

- [ ] **H.1: Apply Tailwind disabled utilities**

  Add `disabled:opacity-50 disabled:cursor-not-allowed` to each disabled control. If any are wrapped in custom components (`Pill`, `Card`), check those forward `disabled`.

- [ ] **H.2: Commit + PR**

**Acceptance:** Disabled inputs/buttons render at ~50% opacity with a not-allowed cursor on hover.

---

## Cross-cutting acceptance check

After all tracks land, re-run the v1 acceptance trace from `v1-shipping-plan.md` §168:

| Criterion | Tracks that close it |
|---|---|
| 1. Authoring | (already ✅; E adds CTA polish) |
| 2. Backtest persists metrics + findings | A |
| 3. Alpaca paper | (already ✅; A adds findings to paper runs too) |
| 4. Compare side-by-side with findings | A (findings) + C (selection UX) |
| 5. `xvn eod` markdown report | (already ✅) |

When all five render cleanly end-to-end on a fresh `XVN_HOME`, the v1 test slice is shippable.

---

## What's next (post v1 close)

- Plan 2c (durable scheduler + live deploy daemon)
- Plan 5 (blockchain — wallet plan + 8004 + Mantle)
- Plan E.2 (`xvn eod` cron registration once 2c lands)
- Plan G (runtime agent rename — wallet-plan-dependent)
- Body-text full-text search (v1.1 follow-up to Plan 12)
- Personalized ranking in the command palette (v1.1)
