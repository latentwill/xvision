# Overnight Eval → Harness Improvements QA Plan

**Date:** 2026-06-08  
**Sources:**
- `docs/QA/2026-06-04-xvnej-user-interaction-findings.md` — real user session mining (F1–F7, open)
- `docs/QA/2026-06-05-autooptimizer-run7-gemini31-findings.md` — run-7 findings (mostly resolved PR #829)
- `docs/QA/2026-06-04-autooptimizer-run5-findings-and-handoff.md` — run-5 findings (mostly resolved PRs #813–#819)
- `docs/superpowers/specs/2026-06-07-control-tower-dashboard-evaluation.md` — control-tower spec (ce-plan)

**Status:** actionable QA list — each item carries an evidence requirement before it can be closed.

**Container restart note:** The run-5 report says "the only way to halt the runaway run was a container restart." That restart was a **scheduled image update** (`docker restart xvn-app` during a routine deploy), not a manually triggered recovery or a recurring workaround. The underlying missing-cancel finding (F28) is fixed in PR #815/#819. No recurring container-restart concern exists.

---

## Part 1 — Harness improvements (by surface)

Items are ordered by severity. Each item requires **evidence before close** as specified.

---

### CLI improvements

#### CLI-1 — Preflight does not resolve the actual model call path (F1)
**Severity:** High  
**Finding:** Preflight logs "passed" and the run fails immediately on the same provider — either 402 (no credits), 404 (model not found), or API rejection. Two gaps: (1) reachability ≠ billable; (2) only 1 provider checked even when the strategy binds a specific provider/model.  
**Files:** `crates/xvision-engine/src/autooptimizer/preflight.rs`, `crates/xvision-engine/src/eval/preflight.rs` (if separate)  
**Work:** Preflight must (a) resolve the *exact* model string the run will call, (b) make a real auth/pricing probe (e.g. a `POST /chat/completions` with 1-token body and handle 402/404 as a preflight failure), (c) repeat for every provider/model the strategy binds — not just the first.  
**Evidence required:** Raw `supervisor_notes` rows from a preflight pass immediately followed by a run error on the same provider must not exist after the fix. A failing test: mock a 402 response from the provider; assert preflight returns `Err` with a user-readable message, not `Ok`.

---

#### CLI-2 — `xvn doctor` does not print the effective config path (F6)
**Severity:** Medium  
**Finding:** The app reads `/data/config/default.toml` (volume) but the host bind-mounts a separate `/config` dir. Operators edit the bind-mounted dir and the change has no effect. `xvn doctor` gives no indication which path is actually being loaded.  
**Files:** `crates/xvision-cli/src/commands/doctor.rs`, `crates/xvision-core/src/config.rs`  
**Work:** Print `effective config path: /data/config/default.toml` (resolved, absolute) in `xvn doctor` output and in startup log at `INFO` level. Optionally print the mtime so the operator can verify freshness.  
**Evidence required:** Run `xvn doctor` against a dev container; output must contain the absolute path of the loaded config file. Screenshot or terminal capture.

---

#### CLI-3 — Eval-start 400 reasons invisible in `api_audit` (F3)
**Severity:** Medium  
**Finding:** `POST /api/eval/runs` returned 400 ≥12 times (user made 6 attempts within 30s). The 400 is rejected at HTTP validation before `eval.start` is emitted to the domain audit log, so the failure reason is invisible.  
**Files:** `crates/xvision-dashboard/src/routes/eval_runs.rs`, `crates/xvision-engine/src/api/eval.rs`  
**Work:** (1) Emit a domain audit event for validation-rejected eval starts, including the rejection reason. (2) Return the validation reason in the 400 response body so the dashboard can surface it. (3) Check whether the form itself is easy to submit with an invalid payload (is there a field that looks filled but produces a 400?).  
**Evidence required:** Reproduce a 400 scenario; verify `api_audit` has an `eval.start_rejected` row with the reason. Verify the 400 response body contains the reason string.

---

#### CLI-4 — Route namespace confusion (`/api/strategy` vs `/api/strategies`) (F4)
**Severity:** Low  
**Finding:** A client hit 405 on `PATCH/PUT/POST /api/strategy/<id>` (singular) before finding the working plural path. The singular 405 gives no hint.  
**Files:** `crates/xvision-dashboard/src/routes/strategies.rs`, `crates/xvision-dashboard/src/server.rs`  
**Work:** For the singular 405 path, return a response body: `{"error": "did you mean /api/strategies/<id>?"}`. Already done for the id-namespace mismatch (`strategy.get` → agent id hint) — extend the same pattern here. Alternatively, add an alias route that redirects singular to plural.  
**Evidence required:** `curl -X PATCH http://localhost:8788/api/strategy/<any-id>` returns a 405 body containing the plural hint. Test case in route tests.

---

### WEBUI improvements

#### WEBUI-1 — One malformed provider entry breaks the entire providers surface (F2)
**Severity:** High  
**Finding:** A single provider with `name = "Gemini"` (capital letter, fails `[a-z0-9-]+`) caused 80% of all API errors over 22 hours — 72 of 90 `providers.list` calls returned a config validation error. The user could not see any providers and was also blocked from re-adding it (409 conflict). Fixed only by manual config edit.  
**Files:** `crates/xvision-engine/src/api/settings/providers.rs` (or equivalent), `crates/xvision-dashboard/src/routes/settings.rs`  
**Work:** Validate providers per-entry at read time. Return the valid providers with per-row error annotations on bad entries (e.g. `{"ok": [...], "errors": [{"index": 5, "name": "Gemini", "error": "name must match [a-z0-9-]+"}]}`). Do not fail the entire list because of one invalid row.  
**Evidence required:** Inject a provider with `name = "Gemini"` into `default.toml`. `GET /api/settings/providers` must return the remaining valid providers and an error annotation on the bad entry, with HTTP 200. Screenshot of the providers UI still rendering the valid ones.

---

#### WEBUI-2 — Run status inconsistency: parent failed, child step completed (F5)
**Severity:** Medium  
**Finding:** `agent_runs` parent row is `failed`, but `agent_runs/<id>::trader::cycle0` child row is `completed`. Step-level dashboards over-report success.  
**Files:** `crates/xvision-engine/src/agent/` (run store, step completion path), `crates/xvision-dashboard/src/routes/agent_runs.rs`  
**Work:** When a parent run transitions to `failed` because a step did not complete, mark that step's row `failed` as well (or `partial`). A completed step that belongs to a failed parent must not show as `completed` in any list or detail view.  
**Evidence required:** Reproduce a trader failure. `SELECT status FROM agent_runs WHERE id LIKE '%::trader::cycle0'` returns `failed` (or `partial`), not `completed`. Test case asserting step status propagation on parent failure.

---

#### WEBUI-3 — `full_debug` retention enabled on shared QA node (F7)
**Severity:** Low  
**Finding:** xvnej startup log warns `full_debug retention enabled — prompts/responses/tool payloads may be stored.` xvnej is a shared QA node; this exposes all users' prompts to disk.  
**Files:** `crates/xvision-observability/` (retention config), `crates/xvision-dashboard/wiki/operator-manual.md`  
**Work:** (1) Add a `xvn doctor` warning when `full_debug` is set and no single-user indicator is present (or just always warn if full_debug is enabled). (2) Add an explicit note to the operator manual about this setting and shared nodes. (3) Optionally: document the route/setting to change retention without a config file edit.  
**Evidence required:** `xvn doctor` output on a node with `full_debug=true` contains a visible warning. MANUAL.md contains the retention setting guidance.

---

### SKILLS improvements

#### SKILL-1 — `xvision-cli-qa` skill should document provider preflight limitation and fix
**Finding (from F1):** The QA skill currently doesn't tell the eval agent that a preflight pass does not guarantee the run will succeed. After WEBUI-1/CLI-1 land, update the skill to explain the improved preflight semantics and what a "pass" now means (model resolved + auth verified per-bound-provider).  
**File:** `.claude/skills/xvision-cli-qa/SKILL.md`  
**Work:** Add a section: "Provider preflight — what a pass means" explaining that preflight now resolves the exact model + does a real auth probe per bound provider/model. If an agent sees a preflight pass followed by a provider error, that is a regression, not expected behavior.  
**Evidence required:** The skill mentions predictive preflight and the specific error classes it now catches (402, 404/model-not-found). Diff shown.

---

#### SKILL-2 — `xvision-cli-qa` skill should document eval-start 400 debugging path
**Finding (from F3):** An eval agent making repeated 400 errors on eval start has no path to diagnose the rejection without the fix in CLI-3. After CLI-3 lands, update the skill to say: check `api_audit` for `eval.start_rejected` rows; the `detail` field contains the validation error.  
**File:** `.claude/skills/xvision-cli-qa/SKILL.md`  
**Work:** Add a "Debugging eval-start failures" subsection: "If `POST /api/eval/runs` returns 400, check `api_audit` for `eval.start_rejected` rows to get the rejection reason. Common causes: …"  
**Evidence required:** The skill has the `eval.start_rejected` audit event name and common root causes listed. Diff shown.

---

#### SKILL-3 — `xvision-cli-qa` skill should explain config path resolution
**Finding (from F6):** A QA eval agent might edit the wrong config file. After CLI-2 lands, the skill should tell agents to use `xvn doctor` output to confirm the effective config path before editing.  
**File:** `.claude/skills/xvision-cli-qa/SKILL.md`  
**Work:** Add "Config location" note: "Always use `xvn doctor` to confirm the effective config path before editing. The app reads from the resolved path (e.g. `/data/config/default.toml` inside the container), which may differ from the bind-mount on the host."  
**Evidence required:** Skill contains the `xvn doctor` guidance for config path. Diff shown.

---

#### SKILL-4 — `xvision-cli` skill — note that autooptimizer cancel route now exists
**Finding:** The autooptimizer cancel route (`POST /api/autooptimizer/cycles/:id/cancel`) was added in PR #815. The `xvision-cli` skill may still describe or imply the optimizer is uncancellable. Update to reflect the current state.  
**File:** `.claude/skills/xvision-cli/SKILL.md`  
**Work:** Verify the skill's optimizer section mentions the cancel route. If it implies "the only way to stop a run is to restart," remove that text. Add the cancel command (`xvn optimizer cancel <cycle-id>` or the HTTP route).  
**Evidence required:** Skill does not say "restart" as the only stop mechanism. It references the cancel route. Diff shown.

---

### Actual harness improvements

#### HARNESS-1 — The QA eval agent script needs a minimum-window guard for BTC/ETH/SOL 1h
**Finding (from run-7):** A QA agent created a 30-day daily scenario to test a strategy, hit the 200-bar warmup minimum, and couldn't run the eval. This is a recurring trap — the `qa-xvision-30day-eval.md` report was blocked by exactly this. The same trap appears in `qa-xvision-30day-eval.md` (Friction #2).  
**File:** `qa-xvision-30day-eval.md` (existing report), the eval agent briefing (update it), `.claude/skills/xvision-cli-qa/SKILL.md`  
**Work:** Add a pre-flight check to the QA eval agent prompt: "Before creating a scenario, verify the date range provides ≥231 bars (200 warmup + 31 post-warmup). For BTC/ETH/SOL 1h, use at least 10 calendar days. For daily candles, use at least 231 calendar days. Use `xvn scenario create --validate` (if available) or calculate manually."  
**Evidence required:** The QA skill's eval section contains the bar-count formula and minimum-window recommendations for the two most common granularities (1h, 1d).

---

#### HARNESS-2 — Container restart complaints from eval agents = likely scheduled image update
**Finding:** The run-5 report says the runaway cycle was halted by a "container restart." The user confirms this was a **scheduled image update** — the container bounced as part of normal ops, not a manual intervention to stop a runaway. After F28's cancel route landed in PRs #815/#819, the correct stop mechanism is the cancel route, not a restart.  
**File:** `.claude/skills/xvision-cli-qa/SKILL.md`  
**Work:** Add a note: "If a QA session is interrupted by a container restart or service bounce, this is likely a scheduled image update — check `docker ps` for the image sha and compare to the previously noted sha. It is NOT a signal that the optimizer or eval has a crash loop or needs manual intervention. Resume the session by re-probing `GET /api/health` and starting a new cycle."  
**Evidence required:** The skill contains this note. Diff shown.

---

## Part 2 — Control Tower QA checklist (ce-plan)

**Source:** `docs/superpowers/specs/2026-06-07-control-tower-dashboard-evaluation.md` (approved spec, passed 3-reviewer adversarial gate)  
**Sequencing:** Slice 1 → Slice 2 → Slice 3 (dependencies between slices; Slice 1 is frontend-only)

**Evidence requirement:** Every QA item requires one of the following as evidence before it can be marked done:
- A screenshot of the specific UI state being verified (for visual/layout items)
- Raw HTTP response (for API items)
- `grep -rn` output (for terminology/text items)
- A test name + `cargo test` or `pnpm test` output (for code correctness items)

---

### Slice 1 — Cockpit reorder (frontend-only PR, no new APIs needed)

The home page at `frontend/web/src/routes/home.tsx` is a setup-completeness panel. These items reshape it into a cockpit.

#### CT-S1-1 — Count cards removed
**What:** Remove the 3 `CountCard` components (strategy / agent / provider counts).  
**Evidence:** `grep -rn "CountCard" frontend/web/src/routes/home.tsx` returns 0 results. Screenshot of home page without count cards.

#### CT-S1-2 — Safety-pause top banner appears when safety is paused
**What:** When `GET /api/safety/state` returns `paused: true`, a top banner ("Safety: system paused") appears above all other content. When not paused, banner is absent.  
**Evidence:** Screenshot with safety paused (force-pause via `xvn safety pause` or direct API call). Screenshot with safety not paused (banner absent). Confirm no false-positive banner on normal loads.

#### CT-S1-3 — Active tasks strip renders queued/running evals
**What:** The home page shows a strip of queued + running eval runs with progress %, ETA (when real), and a Cancel button (only for human-queued evals).  
**Evidence:** Start a long-running eval. Screenshot of home page showing it in the active tasks strip with progress. Cancel it from the strip. Screenshot showing it removed.

#### CT-S1-4 — Active tasks strip renders active live/paper runs
**What:** If `agent_runs` has running live or paper runs, the strip shows them (mode, last decision, unrealized P&L if available from SSE).  
**Evidence:** Screenshot with an active agent run showing in the strip. Screenshot with no active runs showing an empty/hidden strip.

#### CT-S1-5 — Stuck-queue warning appears after 2h no progress
**What:** An eval run that has been queued or running for >2h without progress change shows a warning indicator.  
**Evidence:** Inject a synthetic stale eval run (or mock the endpoint); screenshot showing the warning flag on that run.

#### CT-S1-6 — Critical-findings row shows severity=critical findings with "Draft variant from this"
**What:** `eval_findings` rows with `severity = 'critical'` appear on home as finding cards. Each card has a "Draft variant from this →" action (even if stubbed as a nav link to `/authoring` for now).  
**Evidence:** Screenshot of home page with at least one critical finding visible. Confirm the action link navigates without 404.

#### CT-S1-7 — Per-strategy outcomes list: evaluated strategies show return + Sharpe + max drawdown
**What:** For each strategy that has completed eval runs, show the stored metrics (return, Sharpe, max drawdown) from the most recent `eval_runs` row. Color-code green (return > 0 AND Sharpe > 1.0), neutral otherwise. Below n=10 runs: no coloring, just raw metrics.  
**Evidence:** Screenshot showing strategies with metrics. Confirm a strategy with <10 runs shows plain metrics with no color coding. Raw HTTP `GET /api/eval/summaries?strategy_id=<x>` showing the data used.

#### CT-S1-8 — Un-evaluated strategies appear as "no evals yet"
**What:** Strategies with no completed eval runs appear in the list with "no evals yet" and a link to `/eval-runs/new?strategy=<id>`.  
**Evidence:** Screenshot showing a strategy with no evals. Confirm the link navigates to the new eval form pre-populated with that strategy.

#### CT-S1-9 — Nag strip demoted to bottom, low-contrast
**What:** Config nags (missing provider keys, broker not configured, stale failed runs) appear at the bottom of the home page in a low-contrast muted strip — not at the top and not in the attention section.  
**Evidence:** Screenshot of home page with at least one nag condition active, showing it below the active tasks and outcomes content.

#### CT-S1-10 — No right-side boxes, no popups (QA30 rule)
**What:** The home page uses single-column full-width stacked strips only. No `col-span-4` sidebar, no `Dialog`/`Sheet`/`Popover`.  
**Evidence:** `grep -rn "Dialog\|Sheet\|Popover\|col-span-4\|grid-cols-12" frontend/web/src/routes/home.tsx` returns 0 results. Screenshot on desktop viewport (1440×900) showing correct layout.

#### CT-S1-11 — Terminology: no banned words visible
**What:** The words `setup`, `setups_evaluated`, `StrategyBundle`, `bundle`, `Mutation` (not as "Experiment"), `Mutator` (not as "Experiment writer"), `Ghost` (not as "Rejected"), `Quarantined` (not as "Suspect"), `merkle`, `BLAKE3`, `Ed25519` must not appear in any operator-visible text on the home page.  
**Evidence:** `grep -rn "setup\|StrategyBundle\|bundle\|Mutation\|Mutator\|Ghost\|Quarantined\|merkle\|BLAKE3\|Ed25519" frontend/web/src/routes/home.tsx` returns 0 results (or only in code comments, not render paths). Screenshot of home page passing a visual scan.

---

### Slice 2 — Optimizer digest strip + agent-runs list endpoint (depends on Slice 1 + optimizer P1 landing)

These items require the optimizer-ui-overhaul P1–P5 endpoints (session state, `/cycles`, `/cycles/:id/cost`) to be live.

#### CT-S2-1 — Optimizer digest strip appears when a cycle ran in the last 24h
**What:** A one-line strip: "Last night's run: {kept} kept · {dropped} dropped · {suspect} suspect · honesty check {passed|failed} · ${cost}". Links to `/optimizer/run/:sessionId`.  
**Evidence:** Screenshot with a recent cycle. Raw `GET /api/autooptimizer/status` showing the data used. Screenshot with no recent cycle (strip absent or shows "No recent run").

#### CT-S2-2 — Optimizer digest strip shows budget context for cost
**What:** Cost shows as "$4.10 / $10.00 today" (actual vs budget cap), not just "$4.10", when a budget cap is configured.  
**Evidence:** Screenshot with budget cap set showing the context. `GET /api/autooptimizer/status` response showing both fields.

#### CT-S2-3 — Optimizer digest strip: honesty check tooltip explains what it verifies
**What:** The "honesty check passed/failed" label has a tooltip or info icon that explains what the honesty check is (null-result canary: runs with a sabotaged kill-all-trades signal to verify the model isn't ignoring the prompt).  
**Evidence:** Screenshot of the tooltip. Text of the tooltip contains "kill-trades" or equivalent plain-language explanation.

#### CT-S2-4 — Active tasks strip renders Optimizer cycle if running
**What:** If an optimizer cycle is currently running, it appears in the active tasks strip (with cycle_id, strategy, elapsed time).  
**Evidence:** Start a cycle. Screenshot showing it in the active tasks strip alongside any running evals.

#### CT-S2-5 — `GET /api/agent-runs` list endpoint exists and returns correct shape
**What:** A list endpoint for agent runs (not just individual run detail) must exist for the active tasks strip to render them.  
**Evidence:** Raw HTTP `GET /api/agent-runs?status=running&limit=5` returning a list. If this doesn't exist yet, this is a backend blocker — file it as a separate issue before UI work.

---

### Slice 3 — Verified-outcomes header + capital-risk strip (backend-first; operator decisions pending)

These items depend on operator decisions resolved in the spec (§7.4): live SSE data, no aggregate success-rate number, no snapshot tables.

#### CT-S3-1 — Operator decisions documented before any UI work starts
**What:** Before writing any Slice 3 code, confirm the two operator decisions from §7.4 are still current:
- (a) Outcomes = live SSE data from running evals/strategy runs, NOT snapshot tables
- (b) No aggregate "success rate" — show per-strategy raw metrics (return, Sharpe, max drawdown) colored against the win threshold (return > 0 AND Sharpe > 1.0)  
**Evidence:** A confirmation message from the operator, or this spec's §7.4 treated as the binding decision record. No code written until confirmed.

#### CT-S3-2 — Capital-risk strip is non-negotiable for live-money surfaces
**What:** If any live-money runs are active, the capital-risk strip must appear above all other home content: deployed capital · current drawdown · daily-loss-limit buffer (color-coded). If no live money runs, strip is absent.  
**Evidence:** Screenshot with a live run active showing the strip. Screenshot with only paper/backtest runs showing strip absent. Confirm the drawdown and daily-loss-limit fields are real values, not placeholders.

#### CT-S3-3 — Time-window pills exist and scope the outcomes list
**What:** Today / 7d / 30d / All pills appear and filter the per-strategy outcomes list by the date range of completed eval runs.  
**Evidence:** Screenshot with each pill selected showing different strategy metric values (or fewer strategies shown when switching from All to Today). Raw HTTP showing the filter parameter in the request.

#### CT-S3-4 — `GET /api/eval/summaries` (or equivalent) supports time-window filter
**What:** The list endpoint used by the outcomes strip must accept a `?since=<ISO date>` (or similar) query param. This is currently missing from all list endpoints per §7.1.  
**Evidence:** `GET /api/eval/runs?status=completed&since=2026-06-01T00:00:00Z` returns only runs completed after that date. If the endpoint doesn't exist yet, this is a backend blocker.

#### CT-S3-5 — Deploy-readiness check (candidate panel, not mandatory for Slice 3)
**What:** A "safe to go live?" summary: provider keys ✓/✗ · broker ✓/✗ · no blocking eval failure ✓/✗. This was proposed in the adversarial review (§7.2) but is not listed as mandatory in §7.3.  
**Evidence:** Screenshot of the deploy-readiness check with each of the three gates shown. Confirm a red gate blocks a "Go Live" action (or clearly indicates the user should not proceed).

---

## Execution order

1. **CLI-1, WEBUI-1** first — highest user-visible reliability wins; these are the bugs that caused the most pain in the xvnej session.
2. **CLI-3, CLI-2** — diagnosability improvements.
3. **SKILL-1, SKILL-2, SKILL-3, SKILL-4** — update skills after their corresponding code fixes land so QA agents can use correct guidance.
4. **CT-S1 items** — Slice 1 is frontend-only, can start in parallel with CLI fixes.
5. **HARNESS-1, HARNESS-2** — update the QA eval agent briefing and skills.
6. **CT-S2 items** — after optimizer P1 endpoints land.
7. **CT-S3 items** — after operator confirms S3 scope; backend-first.

---

## What each item needs to close

Every item above has an **Evidence required** line. No item may be closed without that evidence being recorded (screenshot path, HTTP response capture, test output, or grep result). This is enforced at close time — "it looks right" is not sufficient.
