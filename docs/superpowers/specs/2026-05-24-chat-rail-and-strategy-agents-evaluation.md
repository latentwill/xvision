# Chat rail, DSPy, and strategy agents - exhaustive implementation plan

Date: 2026-05-24
Status: implementation plan and acceptance contract

## Purpose

This document converts the earlier chat-rail / strategy-agent evaluation into
an implementation plan with explicit proof gates. It covers two major product
plans and the optimizer foundation between them:

1. Chat rail first.
2. DSPy / DSRs foundation second.
3. Strategy-agent improvement plan third.

The order is intentional. The chat rail is the operator surface that will drive
agent creation, optimization, review, rollback, and evidence capture. DSPy then
lands as an offline optimizer primitive. Only after that should the broader
strategy-agent plan consume the optimizer.

This plan is designed to prevent missed work, partial implementations, hidden
CLI-only behavior, UI-only demos, mock-only proof, and agents silently
short-circuiting when a capability or tool is missing.

## Existing DSPy work to preserve

Do not rediscover these decisions. Fold them into implementation:

- `docs/superpowers/notes/2026-05-21-optimizer-and-capability-framing-handoff.md`
  is the source for the DSRs / rig-core / ClineSDK adapter decision, the
  `AgentSlot.system_prompt` write-back seam, and the capability-not-type model.
- `team/intake/archive/2026-05-21-dspy-dsrs-optimizer-adoption.md` is the
  earlier DSRs adoption intake. Its technical findings remain useful, but its
  old sequencing is superseded by this document. DSPy is now an explicit
  foundation step after the chat rail and before the strategy-agent plan, not a
  sidecar-only filter-v1.5 follow-up.
- `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md`
  declares the runtime boundary: the live runtime consumes an optimized prompt
  snapshot; DSPy does not run in the live decision path.
- `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`
  defines capability-shaped agents. Optimizer adapters attach to capabilities,
  not to a closed `AgentKind` enum.

Locked technical decisions:

- Use native Rust DSRs (`dspy-rs`), pinned.
- Keep DSPy offline. No DSPy call may occur inside a live decision cycle.
- Implement the model bridge at the rig-core completion-model layer backed by
  the same ClineSDK behavior used by runtime sessions.
- Persist optimizer outputs as plain snapshots: instruction string, demos,
  signature hash, metric, corpus query, seed, optimizer version, and lineage.
- Write optimized instructions back through the existing
  `AgentSlot.system_prompt` seam or mint a new child agent. Do not invent a
  parallel prompt source of truth.
- Preserve hand-authored prompts. Optimization is explicit, reviewable, and
  reversible.

## Non-negotiable gates

Every implementation task must close with evidence. A task is not done because
it compiles or because the agent says it is done.

Required evidence per task:

- Code diff scoped to the task.
- Unit or integration test proving the behavior.
- CLI transcript when the behavior is exposed by `xvn`.
- API transcript when dashboard or MCP surfaces depend on it.
- Browser screenshot or Playwright trace for user-visible dashboard changes.
- Before/after JSON for persisted schemas or exported artifacts.
- Migration proof for any schema change: fresh DB, migrated existing DB, and
  rollback or compatibility statement.
- Docs, skill, and script updates when the surface is operator-facing or agent
  facing.

No task can be marked complete with only one surface verified unless the task
is genuinely private implementation plumbing and the plan names it as such.

## Anti-shortcut rules

Agents implementing this plan must not take these shortcuts:

- Do not make a CLI feature without the dashboard, MCP/API, docs, skills, and
  scripts surfaces when the feature is operator-visible.
- Do not make a dashboard-only control that cannot be driven by CLI or API.
- Do not satisfy evidence with mocked data when the feature is supposed to use
  persisted runs, real event streams, or real SQLite rows.
- Do not hide missing agent capabilities behind generic fallbacks that look
  successful.
- Do not emit "done" events until all downstream writes and evidence records
  exist.
- Do not silently skip tools because they are not registered. Missing tools are
  explicit errors with remediation.
- Do not collapse Research / Act safety into a frontend-only toggle. Tool
  enforcement must happen server-side.
- Do not let optimized agents overwrite the parent by default. Mint child
  artifacts unless the operator explicitly accepts an in-place edit.
- Do not add `dspy-rs` to the live runtime image.
- Do not use an LLM judge or live provider call in a required test without a
  deterministic test double and an opt-in live test marker.

## Evidence ledger

Create and maintain an evidence ledger for this wave:

```text
docs/superpowers/evidence/2026-05-24-chat-rail-dspy-strategy-agents/
  README.md
  chat-rail/
    unified-stream-cli.txt
    unified-stream-api.jsonl
    unified-stream-dashboard.png
    research-act-deny-write.txt
    restore-checkpoint.txt
  dspy/
    dependency-spike.txt
    dummy-lm-compile.txt
    optimize-cli-baseline.json
    optimize-cli-candidate.json
    optimization-lineage-row.json
  strategy-agents/
    missing-capability-proof.txt
    optimized-agent-mint.json
    ab-holdout-report.md
    dashboard-agent-diff.png
  final/
    surface-matrix.md
    commands-run.txt
    risk-decisions.md
```

The final PR must link the ledger and summarize which evidence is automated,
which is manual, and which live-provider proof is intentionally opt-in.

## Surface matrix

Each user-visible capability must be represented in every applicable surface.
If a surface is not applicable, record why in the evidence ledger.

| Surface | Chat rail | DSPy foundation | Strategy agents |
|---|---|---|---|
| Dashboard UI | Chat rail rows, modes, approvals, checkpoints, optimizer progress, agent diff | Optimizer run detail, accept/revert, lineage panel | Agent capability status, tune action, missing-feature warnings, A/B result panels |
| CLI | `xvn chat` or equivalent session driver, stream inspect, checkpoint restore | `xvn optimize`, demo import/export, optimization inspect | agent tune/mint/list/inspect, A/B holdout workflow |
| Dashboard API | unified event stream, tool policy, focus chain, checkpoint endpoints | optimization CRUD, corpus preview, demo store, lineage endpoints | capability diagnostics, optimized-agent mint endpoints |
| MCP | chat/session tools where relevant, read-only vs write tools | optimize slot, inspect optimization, export evidence | create/tune/mint agent tools with capability diagnostics |
| Scripts | QA/evidence scripts, event-log diff helpers | optimizer smoke script, corpus export script | A/B and holdout evidence scripts |
| Docs | operator docs, dashboard docs, CLI reference | optimizer design, CLI docs, runbook | agent authoring, marketplace/lineage, evaluation docs |
| Skills | `.claude/skills/xvision*` operator and dev guidance | optimizer usage and contributor guidance | agent authoring QA guidance |
| Tests | frontend, Rust route, engine, CLI | crate tests, migration tests, CLI tests | engine, CLI, dashboard, MCP, regression fixtures |
| Observability | unified event taxonomy and trace IDs | optimization events and cost fields | per-capability metrics and lineage trace |

## Phase 0 - Inventory and proof harness

This is the only preflight before chat rail work. It must not implement product
features. Its purpose is to prevent hidden surface gaps.

Tasks:

1. Inventory existing chat rail, trace dock, agent-run stream, dashboard routes,
   CLI commands, MCP tools, scripts, docs, and skills.
2. Produce a surface map with owners:
   - `frontend/web/src/components/shell/ChatRail.tsx`
   - `frontend/web/src/api/chat_rail.ts`
   - `frontend/web/src/features/agent-runs/TraceDock.tsx`
   - `frontend/web/src/stores/trace-dock.ts`
   - `crates/xvision-dashboard/src/routes/*`
   - `crates/xvision-agent-client/src/*`
   - `crates/xvision-cli/src/*`
   - `crates/xvision-mcp/src/tools.rs`
   - `scripts/*`
   - `.claude/skills/*/SKILL.md`
   - `docs/dashboard.md`, `crates/xvision-dashboard/wiki/*`, `docs/dev/skills/README.md`
3. Add or update an evidence helper script that can:
   - capture an SSE stream to JSONL,
   - assert expected event kinds,
   - redact secrets,
   - write output into the evidence ledger.
4. Add a tracking checklist to the evidence ledger with one row per surface.

Exit evidence:

- `surface-matrix.md` lists every relevant UI, CLI, API, MCP, docs, skill, and
  script touchpoint.
- `scripts/README.md` names the evidence helper.
- `scripts/check_agent_docs.sh` or a new docs check covers the new docs/skill
  files if the current checker is too narrow.

## Phase 1 - Chat rail foundation

Goal: make the chat rail the canonical operator event surface, not a separate
wizard-only stream.

### 1.1 Unified event taxonomy

Implement a single typed event model shared by chat rail and trace dock:

- Session lifecycle: created, resumed, interrupted, completed, failed.
- Assistant output: message started, token delta, content block, message done.
- Tool lifecycle: requested, policy checked, approved, started, delta,
  finished, failed, denied.
- Checkpoints: checkpoint created, restored, restore failed.
- Focus chain: loaded, edited, injected.
- Optimization: candidate started, candidate metric, candidate selected,
  optimization completed.
- Errors: missing capability, missing tool, invalid schema, provider unavailable,
  policy denied, persistence failed.

Required properties:

- Stable `run_id`, `session_id`, `event_id`, `parent_event_id`, `span_id`.
- `scope_kind` and `scope_id`.
- `actor` and `source` fields.
- Redacted payload plus optional blob hash for large content.
- Monotonic sequence number per session.

Evidence:

- Rust serialization round-trip tests.
- Frontend type test or generated type update.
- SSE capture showing chat rail and trace dock consuming the same event stream.

### 1.2 Replace parallel SSE paths

Current issue: `/api/chat-rail/chat` and `/api/agent-runs/:id/stream` have
separate event shapes and consumers.

Plan:

- Make chat sessions attach to the agent-run stream or the same underlying
  event log.
- Keep compatibility shims temporarily, but mark them deprecated in code and
  docs.
- Update `frontend/web/src/api/chat_rail.ts` to consume the unified stream.
- Update `frontend/web/src/stores/trace-dock.ts` so the rail and dock project
  from one source of truth.

Evidence:

- API test for reconnect/resume by `session_id`.
- Frontend test that a tool event appears in both rail projection and dock
  projection without duplicate network streams.
- CLI/API transcript proving stream replay from a finished session.

### 1.3 Session persistence and resume

Add durable chat session records:

- session metadata,
- scope,
- event cursor,
- pinned focus file path,
- participants,
- mode,
- tool policy snapshot,
- checkpoint head.

Acceptance:

- Navigating away and back restores scroll, session state, and event cursor.
- Restarting the dashboard can resume from persisted events.
- If persistence fails, the rail shows a blocking error rather than pretending
  a session exists.

Evidence:

- Route test for session create/resume.
- Browser proof: start session, reload, confirm restored rows.

### 1.4 Per-row streaming instead of bubble mutation cascades

Rows must be event projections with stable IDs:

- assistant row,
- tool row,
- approval row,
- checkpoint row,
- optimizer row,
- error row.

Acceptance:

- Late tool results update only their row.
- Token deltas do not rewrite unrelated rows.
- Duplicate events are idempotent.

Evidence:

- Frontend reducer tests for out-of-order and duplicate events.
- Screenshot of an in-progress tool row and completed row.

## Phase 2 - Chat rail safety, UX, and all surfaces

Goal: make the rail safe enough to drive optimization and agent mutation.

### 2.1 Tool row registry

Build `frontend/web/src/components/chat/tool-rows/` with a registry keyed by
tool name.

Initial rows:

- strategy create/update diff,
- agent slot update diff,
- A/B compare result,
- backtest/eval run status,
- optimizer progress,
- checkpoint restore,
- focus chain edit,
- generic fallback for unknown read-only tools,
- explicit unsupported row for unknown write tools.

Acceptance:

- Unknown read-only tools render as generic but clearly labeled.
- Unknown write tools cannot execute in Act mode until registered with policy.
- New tools require a registry entry or an explicit waiver in tests.

Evidence:

- Component tests for each row.
- Browser screenshot of at least one read row, write approval row, failure row,
  and optimizer progress row.

### 2.2 Research / Act mode

Rename plan/act to Research / Act to avoid trading-domain ambiguity.

Research mode:

- read-only tools only,
- default mode,
- no mutations,
- no strategy, agent, optimizer, checkpoint restore, or marketplace writes.

Act mode:

- write tools available,
- explicit operator transition,
- policy row records who changed mode and when,
- server-side enforcement.

Acceptance:

- A write tool requested in Research mode fails closed with a typed denial.
- Direct API calls cannot bypass the mode.
- UI displays current mode without relying on color alone.

Evidence:

- CLI/API denial transcript.
- Dashboard screenshot.
- Server-side test where frontend mode is spoofed and write still fails.

### 2.3 Three-state tool policy

Persist tool policy as `{ enabled, auto_approve }` scoped by user and
optionally by scope.

Default:

- read tools enabled and auto-approved,
- write tools enabled but require approval,
- dangerous tools disabled until explicitly enabled.

Acceptance:

- Disabled tools are not offered to the model.
- Ask-mode tools block execution until approval.
- Approval/denial events are in the unified stream.

Evidence:

- Migration test.
- API tests for enabled, disabled, ask, and auto-approved.
- Browser proof of approval row.

### 2.4 Focus chain

For each scope, maintain:

```text
$XVN_HOME/scopes/<scope_kind>/<scope_id>/focus.md
```

Acceptance:

- Focus file is loaded on session start.
- Operator edits are detected or explicitly saved.
- The rail shows the focus chain as an editable accordion.
- Inject cadence is recorded in events.

Evidence:

- File-system test with path safety.
- Browser proof of edit and reinjection event.
- Docs update explaining where the file lives.

### 2.5 Checkpoints and restore

Before every mutating tool:

- snapshot Strategy, Agent, AgentSlot, policy, focus chain, and relevant DB rows,
- write content-addressed blobs,
- attach checkpoint hash to the event.

Acceptance:

- Restore rewinds mutated artifacts and chat cursor.
- Restore is unavailable for events without a checkpoint and explains why.
- Restore failures are typed and non-destructive.

Evidence:

- Integration test: mutate, restore, byte-compare artifacts.
- CLI/API transcript of restore.
- Browser proof of restore affordance.

### 2.6 Hook engine

Introduce explicit hook policy:

- mode: blocking or async,
- timeout,
- retries,
- failure mode: fail open or fail closed,
- max concurrency,
- event kinds observed.

Use hooks for evidence capture, policy enforcement, and optional exports.

Acceptance:

- Blocking hooks can deny execution.
- Async hook failures do not lie about primary execution status.
- Hook output is visible in traces.

Evidence:

- Unit tests for timeout and failure-mode behavior.
- SSE capture showing hook events.

### 2.7 CLI, MCP, docs, skills, scripts for chat rail

Required surfaces:

- CLI: session start/resume/inspect, stream capture, mode set, tool policy set,
  checkpoint list/restore.
- MCP/API: equivalent read/write tools for agent-driving clients.
- Docs: dashboard wiki, operator docs, CLI reference.
- Skills: `.claude/skills/xvision-cli/SKILL.md`,
  `.claude/skills/xvision-cli-qa/SKILL.md`, and
  `.claude/skills/xvision-dev/SKILL.md` updated with rail usage and QA rules.
- Scripts: evidence capture helper documented in `scripts/README.md`.

Exit evidence:

- CLI transcript for each new verb or flag.
- MCP tool list or schema snapshot.
- Docs checker output.
- Skill diff included in PR.

## Phase 3 - DSPy / DSRs foundation

Goal: add an offline optimizer primitive that can compile prompts and demos
without entering the live runtime path.

### 3.1 Dependency spike

Before adding permanent product code:

- Pin `dspy-rs` version.
- Confirm workspace compatibility with `tokio`, `rig-core`, and feature flags.
- Run DSRs dummy LM and at least one local compile example.
- Confirm no dependency enters the deploy/runtime image unless explicitly in an
  optimizer image.

Acceptance:

- `cargo tree` proof shows where `dspy-rs` enters.
- A deterministic dummy-LM optimizer smoke test passes without network.
- Live-provider spike is optional and marked as such.

Evidence:

- `dependency-spike.txt`
- `dummy-lm-compile.txt`
- `cargo tree` excerpt

### 3.2 `xvision-dspy` crate

Create an offline crate:

- depends on DSRs and xvision types,
- provides signatures per capability,
- provides metric adapters,
- provides snapshot serialization,
- exposes no live runtime dispatch path.

Acceptance:

- `xvision-engine` does not depend on `dspy-rs`.
- Release runtime image does not include optimizer-only code.
- Tests use deterministic LM doubles.

Evidence:

- crate-level tests,
- `cargo tree -p xvision-engine` proof,
- runtime image or build graph proof if image changes are touched.

### 3.3 ClineSDK / rig-core adapter

Implement the bridge described in the 2026-05-21 handoff:

- rig-core completion model backed by ClineSDK-compatible dispatch,
- deterministic test model for CI,
- provider/model identity recorded in optimizer provenance,
- request/response redaction consistent with xvision observability.

Acceptance:

- Adapter behavior matches the runtime provider path for prompt/message shape.
- Provider unavailability is a typed optimizer error.
- Cost and token accounting are recorded.

Evidence:

- adapter tests,
- redaction test,
- provenance JSON sample.

### 3.4 Signatures and capability registry

Define signatures for at least:

- trader decision,
- filter signal,
- post-hoc decision grader,
- intern briefing if data exists,
- chat rail artifact authoring.

Acceptance:

- Each signature maps to an existing capability contract.
- Each signature has parser/validator boundaries.
- Unsupported capabilities fail with `missing_capability_optimizer` and a
  remediation message.

Evidence:

- signature hash tests,
- validation failure tests,
- capability registry snapshot.

### 3.5 Demo and optimization store

Add migrations for:

- captured demos,
- demo sets,
- optimization runs,
- optimization candidates,
- accepted snapshots,
- parent/child agent lineage,
- evidence blob references.

Acceptance:

- Optimizations are reproducible from corpus query, seed, model, optimizer,
  signature hash, demos, and metric config.
- Migrations include backfill or compatibility behavior.
- Large blobs are content-addressed rather than duplicated inline.

Evidence:

- migration tests,
- sample rows in JSON,
- export/import round trip.

### 3.6 `xvn optimize`

Add CLI:

```bash
xvn optimize \
  --agent <agent_id> \
  --slot <slot_name> \
  --capability <capability> \
  --corpus <query-or-file> \
  --optimizer mipro|gepa|copro \
  --metric <metric> \
  --max-rounds <n> \
  --rng-seed <seed> \
  --dry-run \
  --json
```

Required subcommands or flags:

- inspect optimization,
- export demos,
- import demos,
- accept as child agent,
- revert accepted optimization,
- explain missing data.

Acceptance:

- `--dry-run` validates corpus and capability without mutating.
- No network tests are required for CI.
- Exit codes distinguish missing data, missing capability, provider failure,
  metric failure, validation failure, and persistence failure.

Evidence:

- CLI tests for success and every failure class.
- JSON contract snapshots.
- `xvn optimize --help` captured in docs.

### 3.7 Dashboard and chat rail optimizer surfaces

Expose:

- tune action from agent/strategy detail,
- optimizer progress row in chat rail,
- candidate table,
- before/after prompt diff,
- metric delta,
- holdout split details,
- accept as child agent,
- reject/revert,
- evidence export link.

Acceptance:

- No optimizer jargon is required for a normal operator path. UI can say
  "Improve this agent" while details still expose MIPRO/GEPA for advanced use.
- Long-running optimization does not freeze the rail.
- Failed optimizations preserve partial evidence.

Evidence:

- browser screenshots,
- frontend tests,
- route tests.

### 3.8 Docs, skills, scripts for DSPy

Update:

- `docs/dashboard.md`
- `crates/xvision-dashboard/wiki/agents.md`
- `crates/xvision-dashboard/wiki/cli-reference.md`
- `docs/dev/skills/README.md`
- `.claude/skills/xvision-cli/SKILL.md`
- `.claude/skills/xvision-cli-qa/SKILL.md`
- `.claude/skills/xvision-dev/SKILL.md`
- `scripts/README.md`

Add scripts as needed:

- demo corpus export,
- optimizer smoke test,
- evidence bundle export,
- docs/skills freshness check.

Exit evidence:

- docs checker output,
- skill QA checklist,
- script tests.

## Phase 4 - Strategy-agent improvement plan

Goal: use the chat rail and DSPy foundation to improve strategy agents without
silent missing-feature failures.

### 4.1 Capability completeness audit

Build a capability diagnostics layer that answers:

- Which capabilities does this agent declare?
- Which capabilities are required by the strategy graph?
- Which tools are required by each capability?
- Which prompts, demos, schemas, model bindings, memory inputs, filters, and
  data sources are missing?
- Which capabilities can be optimized now?
- Which capabilities are intentionally unsupported?

Acceptance:

- Missing features are typed statuses, not warnings hidden in text.
- Strategy validation fails or blocks launch when a required capability is
  missing.
- Optional capabilities are shown as optional and do not block.

Evidence:

- engine tests for missing required capability,
- CLI `agent inspect --diagnostics --json` or equivalent,
- dashboard screenshot of capability status.

### 4.2 No-short-circuit execution guardrails

Prevent agents from appearing to work when they skipped real work:

- missing tool,
- disabled tool,
- unavailable provider,
- missing prompt,
- invalid output schema,
- empty demo set when demos are required,
- stale optimized prompt whose signature hash no longer matches,
- filter signal requested but no filter output exists,
- strategy references an agent slot that is not attached,
- dashboard action creates only a UI artifact and no persisted row.

Acceptance:

- Each failure has a distinct code.
- The UI shows remediation.
- CLI returns non-zero with machine-readable JSON when requested.
- Event stream records the failed prerequisite.

Evidence:

- regression tests for each short-circuit class,
- CLI transcripts,
- screenshot of a remediation state.

### 4.3 Strategy-agent tune and mint workflow

Workflow:

1. Operator starts in chat rail or agent detail.
2. System previews corpus and capability readiness.
3. Operator runs optimizer.
4. System shows candidate delta and holdout metrics.
5. Operator mints a child agent or rejects.
6. Child agent is linked to parent and optional marketplace metadata.
7. Strategy can swap to the child agent through a reversible diff.

Acceptance:

- Parent remains unchanged by default.
- Child agent has provenance and evidence.
- Strategy swap is checkpointed.
- Marketplace minting cannot occur without optimization lineage and eval proof.

Evidence:

- end-to-end integration test,
- CLI transcript,
- dashboard screenshots,
- exported child-agent JSON.

### 4.4 Metrics and holdout discipline

Required metrics by capability:

- Trader: forward-return agreement, Sharpe, max drawdown, profit factor,
  calibration, action validity, selectivity, net-of-inference-cost.
- Filter: precision, recall, F1/AUROC, wake rate, token savings, false
  suppression cost.
- Decision grader: Spearman or AUROC against deterministic forward-return
  labels plus rationale quality.
- Chat authoring agent: tool accuracy, schema adherence, mutation safety,
  artifact quality from downstream evals.

Acceptance:

- Train/holdout split is explicit and persisted.
- Optimizer cannot accept a candidate without holdout result unless operator
  uses a documented override.
- Overfit warnings block marketplace minting unless manually waived with a
  recorded reason.

Evidence:

- metrics tests,
- holdout report,
- overfit failure transcript.

### 4.5 UI surfaces for strategy agents

Required dashboard surfaces:

- agent list capability badges,
- agent detail diagnostics tab,
- slot prompt diff and optimized snapshot tab,
- strategy detail agent-readiness panel,
- eval review optimized-vs-parent comparison,
- chat rail tune/mint rows,
- marketplace lineage/evidence panel if marketplace is touched,
- mobile-safe read-only view for diagnostics.

Acceptance:

- The operator can see why a strategy cannot run before launching.
- The operator can see what changed in an optimized agent.
- The operator can revert or swap back.

Evidence:

- browser screenshots for desktop and mobile widths,
- frontend tests,
- route tests.

### 4.6 CLI, MCP, scripts, docs, skills for strategy agents

Required CLI/API:

- inspect agent diagnostics,
- tune agent/slot,
- list optimization history,
- compare parent vs child,
- mint child agent,
- swap strategy slot to child,
- export evidence bundle.

Required MCP tools:

- read diagnostics,
- run dry-run optimization,
- inspect optimization,
- propose mint,
- apply swap only in Act/write context.

Required scripts:

- holdout evidence runner,
- optimized-agent export verifier,
- diagnostics smoke test.

Required docs/skills:

- CLI reference,
- dashboard wiki,
- agent authoring docs,
- operator runbook,
- xvision usage skill,
- xvision QA skill,
- xvision dev skill.

Evidence:

- one transcript per CLI command,
- MCP schema snapshot,
- docs/skill check output,
- script tests.

## Phase 5 - Final evidence, hardening, and release gate

The final merge cannot happen until this checklist is complete:

- [ ] Chat rail event stream is unified or any compatibility shim has a removal
      ticket and a test preventing new callers.
- [ ] Research / Act mode is enforced server-side.
- [ ] Tool policy exists and blocks write tools by default.
- [ ] Checkpoints exist for every mutating chat action.
- [ ] Focus chain exists per scope and is documented.
- [ ] DSPy is offline-only.
- [ ] `xvision-engine` and runtime images do not depend on `dspy-rs`.
- [ ] `xvn optimize` has deterministic tests and documented live-provider
      opt-in proof.
- [ ] Optimized agents are minted with parent lineage by default.
- [ ] Missing capability and short-circuit cases are typed and tested.
- [ ] UI, CLI, API, MCP, docs, skills, and scripts are updated or explicitly
      marked not applicable with rationale.
- [ ] Evidence ledger includes command outputs, JSON artifacts, screenshots,
      and unresolved risk notes.
- [ ] All new migrations are tested from fresh and existing DB states.
- [ ] Frontend has browser verification for the primary flows.
- [ ] Docs freshness checks pass.
- [ ] Final PR body links evidence and names any intentional deferrals.

Recommended validation command set:

```bash
cargo test -p xvision-engine
cargo test -p xvision-cli
cargo test -p xvision-dashboard
cargo test -p xvision-mcp
cargo test -p xvision-agent-client
cargo test -p xvision-dspy
pnpm --dir frontend/web test
pnpm --dir frontend/web build
bash scripts/docs-freshness-lint.sh
bash scripts/check_agent_docs.sh
```

If any command is too expensive or blocked by network/provider dependencies,
record why in `final/risk-decisions.md` and provide the nearest deterministic
substitute.

## Deferral policy

Deferrals are allowed only when they are explicit and evidence-backed.

A valid deferral must include:

- the missing behavior,
- the affected surfaces,
- why it is safe to defer,
- what user action is blocked or degraded,
- a tracking issue or follow-up file,
- a test that prevents accidental silent success.

Invalid deferrals:

- "No UI yet" for an operator-facing feature.
- "No CLI yet" for a dashboard action that mutates durable state.
- "Mock proof only" for a feature whose purpose is real persisted evidence.
- "DSPy can be live later" without preserving the offline-only invariant.

## Implementation order summary

1. Phase 0 inventory and evidence harness.
2. Phase 1 chat rail foundation.
3. Phase 2 chat rail safety and all surfaces.
4. Phase 3 DSPy / DSRs foundation.
5. Phase 4 strategy-agent improvement plan.
6. Phase 5 evidence, hardening, and release gate.

The chat rail must lead because it is the human and agent operating surface.
DSPy must land before the strategy-agent plan because strategy-agent
optimization should consume a proven optimizer primitive. The strategy-agent
plan must come last because it depends on both the rail and optimizer to avoid
partial, unreviewable mutations.

## References

- `docs/superpowers/notes/2026-05-21-optimizer-and-capability-framing-handoff.md`
- `team/intake/archive/2026-05-21-dspy-dsrs-optimizer-adoption.md`
- `docs/superpowers/specs/2026-05-17-cline-sdk-agent-replacement-design.md`
- `docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`
- `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md`
- `docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md`
- `docs/superpowers/plans/2026-05-24-cline-runtime-unification-INDEX.md`
- `docs/superpowers/plans/2026-05-24-cline-stage0-acpx-purge.md`
- `docs/superpowers/plans/2026-05-24-cline-stage1-live-path.md`
- `docs/superpowers/plans/2026-05-24-cline-stage2-trajectory-record.md`
- `docs/superpowers/plans/2026-05-24-cline-stage3-replay-unify-eval.md`
- `docs/superpowers/plans/2026-05-24-cline-stage4-throughput-hardening.md`
- `docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md`
- `docs/superpowers/plans/2026-05-11-agents-page-v1.md`
- `docs/superpowers/plans/2026-05-10-chat-rail-persistence-plan.md`
- `docs/superpowers/specs/2026-05-13-chatrail-file-attach-design.md`
- `docs/superpowers/specs/2026-05-15-chat-strategy-agent-authoring-recovery.md`
- `docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md`
- `docs/cli-non-surfaced.md`
- `docs/dashboard.md`
- `docs/dev/skills/README.md`
- `.claude/skills/xvision-cli/SKILL.md`
- `.claude/skills/xvision-cli-qa/SKILL.md`
- `.claude/skills/xvision-dev/SKILL.md`
- DSRs: `github.com/krypticmouse/DSRs`
- `dspy-rs`: crates.io and docs.rs
- Cline SDK: `github.com/cline/cline` and `docs.cline.bot/sdk`
