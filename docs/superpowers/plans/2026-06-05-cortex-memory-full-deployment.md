# Cortex Memory — Full Deployment Across Agent Surfaces — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Each Phase is an independently-shippable PR; do them in order (Phase 0 → 4) because each later phase depends on Phase 0's substrate readiness.

**Goal:** Make the `xvision-memory` ("Cortex") memory layer a live, operator-controlled capability across all four agent surfaces — strategy agents, the review agent (autooptimizer Judge), the optimizer (mutator), and the chat rail — so agents recall relevant prior experience before acting and write salient outcomes back, with shared learnings across model frameworks.

**Architecture:** The substrate is the **already-built, in-process `xvision-memory` crate** — a SQLite-backed `MemoryStore` (tiers: `Observation` + `Pattern`), an `Embedder` trait (OpenAI adapter today), and a `MemoryRecorder` (recall → render → record) with namespace scoping (`Off` / `Global` / `AgentScoped`) and backtest temporal-safety. **It is NOT the external gambletan/cortex HTTP sidecar** from `docs/superpowers/plans/2026-05-11-cortex-memory-integration-plan.md`; that plan was superseded by this self-contained crate (see "Substrate decision" below). Strategy agents are already wired (recall/record in `execute_slot`) but default to `memory_mode=Off`. This plan provisions the embedder, surfaces operator controls, and wires the three dormant surfaces (Judge, mutator, chat rail) through the same `MemoryRecorder` API, plus the cross-cutting concerns (namespaces, observability, retention, redaction, eval temporal-safety).

**Tech Stack:** Rust workspace (`xvision-engine`, `xvision-dashboard`, `xvision-memory`, `xvision-cli`), SQLite via `sqlx`, embeddings via the `Embedder` trait (OpenAI-compatible HTTP), Vite/React SPA for operator controls, `cargo test` / `pnpm test`.

---

## Substrate decision (read first — resolve before Phase 0)

There are two possible substrates. **This plan assumes (A).** A code agent must confirm with the operator before starting:

- **(A) Built-in `xvision-memory` (RECOMMENDED, this plan):** the SQLite `MemoryStore` already in the runtime image. Zero new infra; only an **embedder** is an external dependency. Tiers are `Observation` (per-decision facts) + `Pattern` (distilled/promoted wisdom). Already deployed in-process — it ships inside the `xvn` binary.
- **(B) External gambletan/cortex sidecar** (the 2026-05-11 plan): a separate `cortex-http` container with a 4-tier model + Bayesian beliefs. Richer, but it is **not deployed** (`docker ps` on the host shows no cortex container) and would require a substrate swap behind the `MemoryStore` API. **Out of scope here**; if the operator wants it, write a separate substrate-swap plan that keeps the `MemoryRecorder` interface stable so Phases 1–4 are unaffected.

**Naming:** "Cortex" in operator-facing copy = this memory capability. In code it is `xvision-memory` / `MemoryStore` / `MemoryRecorder` (do not rename).

---

## Current state (what exists vs. what is dormant)

| Surface | Recall before action | Write-back after | Status |
|---|---|---|---|
| **Strategy agents** (intern/trader/risk/executor) | ✅ `execute_slot` `recall()` → prepends `<prior_observations>` | ✅ `record()` after EndTurn | **Wired**, gated by per-slot `memory_mode` (default `Off`) |
| **Review agent** (autooptimizer `Judge`) | ❌ none | ❌ none | **Dormant** — `run_judge` builds prompt with no memory |
| **Optimizer** (mutator) | ❌ none (F32 avoid-set is lineage-only; DSPy `dsr_prefix` path exists but `dspy_enabled=false`) | ❌ none | **Dormant** |
| **Chat rail** (`wizard_loop`) | ❌ none | ❌ none | **Dormant** — `memory_mode: Default::default()` placeholder only |
| **Embedder** | OpenAI via `OPENAI_API_KEY` (+ `OPENAI_BASE_URL`); none → recall no-ops | — | **Conditional** — absent on the deploy host today |

Key substrate references (verified file:line):
- `crates/xvision-memory/src/store.rs` — `MemoryStore::{open, open_in_memory, query, upsert_observation, upsert_pattern, demote_pattern, forget, undo_forget, hard_delete_expired, count_live_observations, list_live_observation_texts}`.
- `crates/xvision-memory/src/types.rs` — `MemoryMode {Off, Global, AgentScoped}`, `MemoryMode::parse_or_off`, `Namespace::for_mode`, `MemoryItem`, `Tier`.
- `crates/xvision-memory/src/embedder.rs` — `Embedder` trait; `crates/xvision-engine/src/api/openai_embedder.rs` — OpenAI adapter.
- `crates/xvision-engine/src/agent/memory_recorder.rs` — `MemoryRecorder::{new, with_embedder, recall, record}`, `render_recalled_patterns`, `RecallResult`.
- `crates/xvision-engine/src/agent/execute.rs:316-384` (recall+inject), `:626-705` (record). `SlotInput` carries `memory`, `memory_mode`, `agent_id`, provenance.
- `crates/xvision-engine/src/api/mod.rs` — `build_memory_recorder()`, `build_default_embedder()`; `ApiContext.memory_recorder`.
- `crates/xvision-engine/src/agents/store.rs:473-474,570` — `memory_mode` read/write; migration `029_agent_slot_memory_mode.sql`.
- `crates/xvision-engine/src/autooptimizer/judge.rs` (run_judge), `autooptimizer/cycle.rs:473` (judge call), `autooptimizer/dspy_flywheel.rs:53` (`query_dsr_prefix`).
- `crates/xvision-dashboard/src/wizard_loop.rs:838-859` (`system_prompt`), `:925-1000` (`run_one_turn`); `routes/chat_rail.rs:314-556` (chat handler); `routes/memory.rs:53-68` (`resolve_store`).

---

## File Structure (created/modified by this plan)

**Phase 0 — substrate readiness**
- Modify: `crates/xvision-engine/src/api/mod.rs` — embedder provisioning (provider-config-aware, not just `OPENAI_API_KEY`), startup diagnostics.
- Create: `crates/xvision-engine/src/agent/local_embedder.rs` — optional deterministic/local embedder fallback (only if operator chooses no-OpenAI).
- Modify: `docker/entrypoint.sh`, `docker-compose*.yml`, `docker/README.md` — `XVN_MEMORY_DB` path on the writable volume + embedder env documentation.
- Modify: `crates/xvision-cli/src/commands/` (memory/doctor verb) — `xvn memory status` / `xvn doctor` reports embedder + store health.

**Phase 1 — strategy agents (enablement + controls)**
- Modify: `frontend/web/src/components/agent/SlotForm.tsx` + `frontend/web/src/api/agents.ts` — surface `memory_mode` control with help copy.
- Modify: `crates/xvision-dashboard/src/routes/` agents route — accept/persist `memory_mode` edits (if not already).
- Test: `crates/xvision-engine/tests/agent_memory_*` (extend), `frontend/web/src/components/agent/SlotForm.test.tsx`.

**Phase 2 — review agent (Judge)**
- Modify: `crates/xvision-engine/src/autooptimizer/judge.rs` — `run_judge` gains optional `MemoryRecorder` + namespace; recall-inject before dispatch; return findings.
- Modify: `crates/xvision-engine/src/autooptimizer/cycle.rs:473` — thread recorder; record confirmed findings as `Pattern`s post-judge.
- Test: `crates/xvision-engine/tests/autooptimizer_judge_memory.rs` (new).

**Phase 3 — optimizer (mutator)**
- Modify: `crates/xvision-engine/src/autooptimizer/cycle.rs` — record each gated candidate's `(diff, delta, verdict)` as an `Observation`; recall prior optimizer outcomes for the parent's strategy family → feed into the mutator.
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs` — `build_user_payload` accepts a `memory_context: Option<&str>` (rendered prior-outcomes) alongside the existing F32 avoid-set + seed focus.
- Modify: `crates/xvision-engine/src/autooptimizer/dspy_flywheel.rs` — ensure `query_dsr_prefix` namespace aligns; optionally enable the flywheel path.
- Test: `crates/xvision-engine/tests/autooptimizer_mutator_memory.rs` (new).

**Phase 4 — chat rail**
- Modify: `crates/xvision-dashboard/src/state.rs` (or equivalent) — expose `memory_recorder()` on dashboard `AppState`.
- Modify: `crates/xvision-dashboard/src/wizard_loop.rs:838-859` — recall-inject into `system_prompt`; record salient turn facts after a completed turn.
- Modify: `crates/xvision-dashboard/src/routes/chat_rail.rs` — pass recorder + scope-derived namespace into `WizardLoop`.
- Test: `crates/xvision-dashboard/src/wizard_loop.rs` (unit), `crates/xvision-dashboard/tests/chat_rail_memory.rs` (new).

**Cross-cutting**
- Modify: `crates/xvision-observability/` — ensure `memory_recall` / `memory_write` `UnifiedEvent` variants cover the new surfaces; redaction applies to recorded text.
- Modify: retention/janitor wiring (`crates/xvision-engine/src/janitor` / `MemoryStore::hard_delete_expired`) — confirm the grace window sweeps memory too.

---

## Cross-cutting invariants (every phase MUST honor)

- **Backtest/eval temporal-safety is tier-1.** Any recall in an eval/backtest/paper-test context MUST pass `current_scenario_start` to `MemoryStore::query` so future-dated `Pattern`s are excluded (`execute.rs:329` is the reference). The optimizer paper-test (Phase 3) and any Judge recall during a cycle run in eval context — they MUST forward `scenario_start`. Live chat (Phase 4) passes `None`.
- **Memory is best-effort, never a hard dependency.** No embedder, store-open failure, or recall error must ever fail the agent/turn/cycle. Every call site degrades to "no recall / no record" + an observability log. Mirror `RecallResult::{Skipped, NoEmbedder, Hits}`.
- **Default Off.** Every new surface defaults to memory disabled and is opt-in (per-slot `memory_mode`, a cycle/chat config flag, or a workspace setting). No surface silently starts writing memory.
- **Redaction before write.** Recorded text routes through the observability redactor (full_debug vs redacted retention) — see `xvision-observability` redactor. Never persist raw secrets/keys into memory items.
- **Namespace discipline.** Use `Namespace::for_mode` for agent surfaces. New surfaces use explicit, documented namespaces (Phase 2/3/4 each define theirs). Do NOT collide the optimizer/judge namespaces with agent `Global`.
- **No DSPy/rig in engine/dashboard.** The memory path is plain `xvision-memory` + `Embedder`; it must not pull `dspy-rs`/`rig-core` into `cargo tree -p xvision-engine`/`-p xvision-dashboard` (locked invariant). The DSPy flywheel stays in `xvision-dspy` (offline-only).

---

## Phase 0 — Substrate readiness (embedder + deploy + health)

**Why first:** every other phase is a no-op without an embedder, and the store path must live on the writable `/data` volume (see the provider-config-volume ownership bug, QA 2026-06-05 — same class of issue).

**Files:**
- Modify: `crates/xvision-engine/src/api/mod.rs` (`build_default_embedder`, `build_memory_recorder`)
- Create: `crates/xvision-engine/src/agent/local_embedder.rs` (optional)
- Modify: `docker/entrypoint.sh`, `docker-compose.yml`, `docker/README.md`
- Modify/Create: `crates/xvision-cli/src/commands/memory.rs` (`xvn memory status`)
- Test: `crates/xvision-engine/tests/memory_embedder_provisioning.rs`

- [ ] **Step 0.1: Decide embedder source (operator question).** OpenAI-compatible (reuse a registered provider's key — e.g. the openrouter/deepseek/openai key already in `secrets/providers.toml`) vs. a local deterministic embedder for offline/no-OpenAI installs. Document the decision at the top of `local_embedder.rs` or in `docker/README.md`.

- [ ] **Step 0.2: Write a failing test for provider-aware embedder build.** `tests/memory_embedder_provisioning.rs`: given a config with a registered openai-compat provider + a key in the secrets store, `build_default_embedder(&ctx)` returns `Some(embedder)`; given no key and no local fallback, returns `None` (recorder still constructs, recall no-ops). Assert the recorder's recall returns `RecallResult::NoEmbedder` when embedder is `None`.

- [ ] **Step 0.3: Run it — expect FAIL** (`build_default_embedder` currently keys only off `OPENAI_API_KEY`).

- [ ] **Step 0.4: Implement provider-aware embedder provisioning.** In `api/mod.rs`, make `build_default_embedder` look up an embedding-capable provider in this order: `XVN_MEMORY_EMBEDDER_PROVIDER` env → a registered openai-compat provider with an embeddings endpoint → `OPENAI_API_KEY` → local fallback (if enabled) → `None`. Keep the OpenAI adapter (`openai_embedder.rs`) as the default impl; honor `OPENAI_BASE_URL` so an openrouter/proxy key works. Do NOT hard-fail on absence.

- [ ] **Step 0.5: (Optional) local embedder fallback.** If Step 0.1 chose offline support, implement `local_embedder.rs` (e.g. a hashing/bag-of-tokens deterministic embedder) behind the `Embedder` trait. Mark clearly that it is low-quality and for offline/dev only. Gate behind `XVN_MEMORY_EMBEDDER=local`.

- [ ] **Step 0.6: Persist the store on the writable volume.** Confirm `XVN_MEMORY_DB` resolves under `$XVN_HOME` (`/data`) — NOT a read-only path. In `entrypoint.sh`, default `XVN_MEMORY_DB="$XVN_HOME/memory.db"` and ensure it is writable by the runtime user (same uid-ownership care as the config volume — see `project_provider_config_volume_ownership`). Add a self-heal/`chown`-equivalent note.

- [ ] **Step 0.7: Add `xvn memory status` (and fold into `xvn doctor`).** Reports: store path + writable?, item counts (`count_live_observations` per namespace), embedder present + id, grace-window days. Wire into the existing CLI command registry. Test: a unit test asserting the status struct serializes the expected fields.

- [ ] **Step 0.8: Run the full test + `xvn memory status` against a temp `$XVN_HOME`. Commit.**
  ```bash
  scripts/cargo test -p xvision-engine --test memory_embedder_provisioning
  git add -A && git commit -m "feat(memory): provider-aware embedder + writable store path + xvn memory status"
  ```

**Phase 0 acceptance:** on the deploy host, `xvn memory status` shows a writable store and a present embedder (or a clear "no embedder — recall disabled" message); strategy-agent recall works when a slot's `memory_mode` is enabled. No agent path regresses when the embedder is absent.

---

## Phase 1 — Strategy agents: operator enablement + controls

**Why:** the wiring exists; what's missing is operator visibility/control and verification end-to-end. Minimal code, high leverage.

**Files:**
- Modify: `frontend/web/src/components/agent/SlotForm.tsx`, `frontend/web/src/api/agents.ts`, `frontend/web/src/api/types.gen/AgentSlot.ts` (regenerated)
- Modify: dashboard agents route handler (persist `memory_mode` — confirm it round-trips)
- Test: `frontend/web/src/components/agent/SlotForm.test.tsx`; extend `crates/xvision-engine/tests/agent_memory_*`

- [ ] **Step 1.1: Confirm round-trip.** Write/extend an engine test: save an `AgentSlot` with `memory_mode = AgentScoped`, reload via `AgentStore::load`, assert it persists (guards migration 029 + `store.rs:473-474,570`). Run — should already pass; if not, fix the route/store.

- [ ] **Step 1.2: Failing UI test.** `SlotForm.test.tsx`: the form renders a "Memory" control with options Off / Global / Agent-scoped and submits `memory_mode` in the agent payload. Assert it's absent today (fails).

- [ ] **Step 1.3: Add the control.** In `SlotForm.tsx`, add a select bound to `memory_mode` (Off default) with operator copy: "Off = no memory. Global = shares learnings across all agents. Agent-scoped = this agent only." Respect the no-popups + dark-mode-border rules. Thread through `agents.ts` create/update payloads.

- [ ] **Step 1.4: Run UI test (pass) + `pnpm typecheck`. Commit.**

- [ ] **Step 1.5: End-to-end verification doc.** Add a short runbook to `MANUAL.md` (operator-surface): enable memory on a trader slot, run two eval cycles, confirm `xvn memory status` shows growing observations and the second run's trace shows a `memory_recall` event. (No code; verification steps.)

**Phase 1 acceptance:** an operator can enable memory per agent from the dashboard; an enabled trader recalls prior observations across eval runs (visible in the trace dock as `memory_recall`), with backtest temporal-safety intact.

---

## Phase 2 — Review agent (autooptimizer Judge): recall + write-back

**Why:** the Judge reviews every gated candidate but starts cold each time. Memory lets it recall prior findings about similar mutations and persist new ones.

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/judge.rs` (`run_judge`)
- Modify: `crates/xvision-engine/src/autooptimizer/cycle.rs:473` (call site + post-judge write-back)
- Test: `crates/xvision-engine/tests/autooptimizer_judge_memory.rs`

- [ ] **Step 2.1: Define the namespace.** `autooptimizer:judge` (Global to the workspace — findings generalize across strategies). Document it in a `const JUDGE_MEMORY_NS: &str` in `judge.rs`.

- [ ] **Step 2.2: Failing test.** `autooptimizer_judge_memory.rs`: with a `MemoryRecorder` seeded with a Pattern ("raising leverage past 3x degraded holdout Sharpe"), `run_judge(..., Some(&recorder))` for a leverage-raising diff includes a `<prior_observations>` block in the dispatched system prompt (capture via a prompt-recording dispatch). Assert it's absent today (compile-fails — new param — then assert content).

- [ ] **Step 2.3: Add recall to `run_judge`.** New optional params `memory: Option<&MemoryRecorder>`, `scenario_start: Option<DateTime<Utc>>`. Before building the `LlmRequest`, recall (`k=3`, query = parent name + serialized diff + objective) and prepend `render_recalled_patterns(&matches)` to `JUDGE_PROMPT`. Forward `scenario_start` (cycles run in eval context → temporal-safety). Degrade silently on `Skipped`/`NoEmbedder`.

- [ ] **Step 2.4: Thread the recorder at the call site.** `cycle.rs:473` — pass the cycle's recorder (from `ApiContext.memory_recorder` / a new `CycleConfig`/orchestrator field) and `cycle_config.day_scenario` start.

- [ ] **Step 2.5: Write-back.** After `run_judge` returns, for each confirmed finding, `record()` a concise Pattern-candidate Observation into `autooptimizer:judge` (text = `code + summary`, provenance = cycle_id). Gate behind the same memory-enabled flag. (Promotion Observation→Pattern is the flywheel's job; do not auto-promote here.)

- [ ] **Step 2.6: Run tests (recall present; write-back persists) + full autooptimizer suite. Commit.**

**Phase 2 acceptance:** with memory enabled, the Judge's prompt carries relevant prior findings; new findings persist to `autooptimizer:judge`; cycles with memory disabled behave exactly as today; temporal-safety holds.

---

## Phase 3 — Optimizer (mutator): cross-run/cross-framework learning

**Why:** this is the operator's headline ask — the F32 avoid-set only prevents *exact* re-derivation on one parent. Memory lets the mutator learn "this *class* of change failed" across parents, runs, strategies, and model frameworks (the candidate is content-addressed, so a gemini-run's lesson informs a claude-run).

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/cycle.rs` (record outcomes; recall prior outcomes)
- Modify: `crates/xvision-engine/src/autooptimizer/mutator.rs` (`build_user_payload` accepts rendered memory context)
- Modify: `crates/xvision-engine/src/autooptimizer/dspy_flywheel.rs` (namespace alignment; optional enablement)
- Test: `crates/xvision-engine/tests/autooptimizer_mutator_memory.rs`

- [ ] **Step 3.1: Namespace.** `autooptimizer:mutations` (Global). Document. Note its relationship to the existing F32 `avoid` set (lineage hashes) — memory is the *generalized, cross-run* layer; the avoid-set remains the *hard, exact-repeat* guarantee. Both stay.

- [ ] **Step 3.2: Failing test — record outcomes.** `autooptimizer_mutator_memory.rs`: after a cycle gates a candidate (Pass/Fail with `delta_sharpe`), an Observation is written to `autooptimizer:mutations` with text encoding the change (key/direction) + outcome. Assert absent today.

- [ ] **Step 3.3: Record gated outcomes.** In `cycle.rs::process_parent_mutations`, after `gate_and_classify`, `record()` an Observation: text = a compact description of the diff (e.g. `param stop_loss_atr_multiple 2.0→3.5 ⇒ ΔSharpe -0.4 (rejected)`), provenance = cycle_id. Forward `scenario_start` (eval context). Gate behind the memory-enabled flag.

- [ ] **Step 3.4: Failing test — recall informs the mutator prompt.** Seed memory with prior outcomes; assert the mutator's user payload contains a "prior optimizer outcomes" section listing them (capture via prompt-recording dispatch). This is the "at least inform" enrichment the operator asked for — now backed by cross-run memory, not just the current parent's children.

- [ ] **Step 3.5: Thread recall into `propose`.** Before the retry loop in `Mutator::propose` (or computed by the orchestrator and passed in), recall `k=5` from `autooptimizer:mutations` (query = parent program-view summary). Render into a `memory_context: Option<String>` and pass to `build_user_payload`, which appends a "Prior outcomes on similar strategies (avoid repeating failures, build on wins)" section — alongside the existing F32 avoid-set count + seed-directed focus. Keep it advisory; the hard `already_tried` reject still governs exact repeats.

- [ ] **Step 3.6: (Optional) enable the DSPy flywheel path.** If the operator wants distilled demonstrations, align `query_dsr_prefix`'s namespace with `autooptimizer:mutations` and document flipping `dspy_enabled` + wiring a `DspyContext` into `run_cycle` (today dashboard/CLI pass `None`). Keep DSPy offline-only — do not import it into the engine; the flywheel reads/writes the same `MemoryStore`.

- [ ] **Step 3.7: Run tests + full autooptimizer suite (incl. F32 diversity tests still green). Commit.**

**Phase 3 acceptance:** with memory enabled, the mutator's prompt is informed by prior optimizer outcomes spanning runs/strategies/frameworks; the F32 hard-dedup + seed-focus guarantees are unchanged; temporal-safety holds; a memory-disabled cycle is byte-for-byte the current behavior.

---

## Phase 4 — Chat rail: recall + write-back

**Why:** the chat rail is the operator's primary interface; memory makes it remember context across sessions within the same scope (strategy/run/workspace).

**Files:**
- Modify: dashboard `AppState` — expose `memory_recorder()` (build at startup like the engine does, or reuse `routes/memory.rs::resolve_store` + an embedder)
- Modify: `crates/xvision-dashboard/src/wizard_loop.rs:838-859` (`system_prompt`), `:925-1000` (turn completion)
- Modify: `crates/xvision-dashboard/src/routes/chat_rail.rs:314-556` (thread recorder + namespace)
- Test: `crates/xvision-dashboard/tests/chat_rail_memory.rs`, `wizard_loop.rs` unit

- [ ] **Step 4.1: Namespace from scope.** `chat:{scope_kind}:{scope_id}` (e.g. `chat:strategy:<draft_id>`, `chat:run:<run_id>`, `chat:workspace`) — scope-based so memory survives session deletion. Add a helper `ContextScope::memory_namespace(&self) -> String`. Document why scope-based, not session-based.

- [ ] **Step 4.2: Expose the recorder on `AppState`.** Build a `MemoryRecorder` at dashboard startup (reuse Phase 0's embedder provisioning) or lazily via `OnceCell` (as `routes/memory.rs` already does for the store). Default the chat memory mode to Off behind a workspace setting `XVN_CHAT_MEMORY=1` (or a settings toggle).

- [ ] **Step 4.3: Failing test — recall injects.** `chat_rail_memory.rs`: with memory enabled + a seeded Pattern for `chat:strategy:s1`, a turn in that scope prepends a `<prior_observations>` block to the system prompt (capture via the existing prompt assembly in `system_prompt()`). Assert absent today.

- [ ] **Step 4.4: Recall in `system_prompt()`.** In `wizard_loop.rs:838-859`, after the scope/profile sections and before the focus section, if chat memory is enabled, recall (`k=5`, query = latest user message, `scenario_start=None` — live context) from the scope namespace and prepend the rendered block. Degrade silently when disabled/no-embedder.

- [ ] **Step 4.5: Failing test — write-back.** After a completed turn (`WizardEvent::Done`), a salient Observation is recorded to the scope namespace. Assert absent today.

- [ ] **Step 4.6: Write-back after a completed turn.** In `chat_rail.rs` after the turn drains and `Done` is emitted (≈`:536-548`), record a concise Observation (e.g. user-intent + assistant-conclusion summary) to the scope namespace, redacted, behind the enabled flag. Live context → no `training_window_end`.

- [ ] **Step 4.7: Run tests + `pnpm typecheck` (no SPA change needed) + full dashboard build. Commit.**

**Phase 4 acceptance:** with chat memory enabled, a new session in a known scope recalls prior salient facts; turns record back; Research/Act gating and the no-popups/layout rules are unaffected; disabled = today's behavior.

---

## Cross-cutting closeout (do alongside the phases, verify at the end)

- [ ] **Observability:** confirm `memory_recall` / `memory_write` `UnifiedEvent`s fire on all four surfaces and render in the trace dock; add per-surface labels. Keep the adjacently-tagged `{kind,data}` shape; mirror the TS type.
- [ ] **Redaction:** every `record()` text passes through the observability redactor under `redacted` retention; add a test that a secret-shaped token is not persisted.
- [ ] **Retention/janitor:** confirm `MemoryStore::hard_delete_expired` runs under the existing janitor with the grace window (`XVN_MEMORY_FORGET_GRACE_DAYS`, default 14); `xvn memory undo-forget` restores within grace.
- [ ] **Temporal-safety audit:** grep every new `query()` call; assert eval-context callers (Phases 2 & 3) forward `scenario_start`, live caller (Phase 4) passes `None`. Add a regression test per surface.
- [ ] **Dependency guard:** `cargo tree -p xvision-engine | grep -E 'dspy-rs|rig-core'` and same for `-p xvision-dashboard` → must be empty.
- [ ] **Deploy:** rebuild image (local build → SSH per the deploy guardrails), set the embedder env on `xvn` (dev) first, run the Phase-1 runbook live, then promote to `xvnej` only on explicit operator approval. Verify `xvn memory status` on the host.

---

## Self-review checklist (completed by plan author)

- **Spec coverage:** all four named surfaces have a phase (strategy agents = P1, review/Judge = P2, optimizer = P3, chat rail = P4) over a shared substrate (P0) + cross-cutting closeout. ✅
- **Substrate reality:** plan corrects the premise — the substrate is built-in `xvision-memory`, not the external cortex sidecar; the sidecar is an explicit out-of-scope alternative. ✅
- **Operator's F32 follow-up:** Phase 3 is exactly the "inform via memory across model frameworks" ask, with the content-addressed cross-framework property called out. ✅
- **Safety:** backtest temporal-safety, best-effort degradation, default-Off, redaction, and the no-DSPy-in-engine invariant are cross-cutting requirements on every phase. ✅
- **Open decisions a code agent MUST resolve first:** (1) substrate A vs B; (2) embedder source (OpenAI-compat key reuse vs local); (3) which surfaces to enable by default vs leave operator-gated. These are flagged inline (Steps 0.1, Substrate decision, default-Off rule).

---

## Notes for the handoff

- Each Phase is a standalone PR; ship P0 first and verify on `xvn` (dev) before P1–P4. P2/P3/P4 are independent of each other given P0.
- Reuse the proven `MemoryRecorder` recall→`render_recalled_patterns`→record shape from `execute.rs` on every new surface rather than inventing per-surface logic — that is the single source of truth for framing, temporal-safety, and observability.
- Related context: `docs/superpowers/plans/2026-05-11-cortex-memory-integration-plan.md` (the superseded sidecar plan, for the tier model/rationale), `docs/superpowers/evidence/2026-05-24-cortex-flywheels/` (the flywheel/distillation evidence), and the F32 fix in `crates/xvision-engine/src/autooptimizer/{mutator,cycle}.rs` (PR `fix/optimizer-run6-diversity-attribution`).
