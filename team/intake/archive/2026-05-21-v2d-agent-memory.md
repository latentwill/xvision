# Intake — 2026-05-21 — agent memory & per-agent toggle (V2D)

This is the V2D intake. It decomposes V2D item 15 (Rust cortex memory +
per-agent memory toggle) from `team/board-v2.md` into named tracks and
records the design decisions the conductor needs locked before any
contract is written.

V2D is the first of two prerequisites for V3 autooptimizer; V2E (eval
accuracy & trace surface) is the second. The autooptimizer's mutator
loop needs persistent agent memory so that judged-bad outcomes don't
get re-discovered every night.

## Revision history

- 2026-05-21 — initial intake; companion to the implementation plan at
  `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`.

## Source

- `team/board-v2.md` V2D notes section — preliminary decomposition and
  the user-stated requirement (memory is a per-slot switch with three
  modes).
- `docs/superpowers/specs/2026-05-11-install-customizer-design.md` §9 —
  reserves the `memory` plugin slot, names the `cortex-http` sidecar,
  declares the `memory.toml` config + `127.0.0.1`-bound privacy posture.
  V2D delivers the in-process variant of that contract; the sidecar
  variant remains a follow-up gated on F28 plugin architecture (see
  "Out of this intake" below for why the sidecar is not v1).
- The previously-referenced
  `docs/superpowers/plans/2026-05-11-cortex-memory-integration-plan.md`
  did not exist before this wave — it is written as part of this intake.

## Current state (what already ships)

Pulled directly from the tree at intake time:

- **No memory crate.** No `xvision-memory` crate exists; `grep -r
  cortex` returns zero hits in `crates/`.
- **No memory column on AgentSlot.** The struct at
  `crates/xvision-engine/src/agents/model.rs:96` carries `name`,
  `provider`, `model`, `system_prompt`, `skill_ids`, `max_tokens`,
  `temperature`, `prompt_version`, `inputs_policy`, `bar_history_limit`
  — and nothing else. The 2026-05-12 strategies refactor froze the
  surface; new fields must follow the same `#[serde(default)]` +
  SQLite-sentinel pattern used by `bar_history_limit` (migration 025).
- **No embedding client.** `crates/xvision-intern/` has chat-completion
  + tool-use dispatch (`OpenAICompatIntern`, `AnthropicIntern`,
  `AcpxIntern`) but no embedding endpoint surface.
- **System prompt assembly.** `crates/xvision-engine/src/agent/llm.rs:902`
  is the OpenAI-compat wire boundary; `:665` is the Anthropic boundary.
  Both currently treat `LlmRequest.system_prompt` as a single string.
  Any memory prefix has to be assembled *before* the request hits
  either dispatcher — i.e. one seam in `execute_slot` or upstream of
  it, not two seams in each provider.
- **Eval review.** `crates/xvision-engine/src/eval/` writes
  `decisions.jsonl` and `events.jsonl` per run; the eval-review UI
  reads these through the `eval-review-run-detail-ui` track. There is
  currently no `memory_*` event kind — the eval review surface treats
  every decision as memoryless.
- **Migrations.** Engine migration directory at
  `crates/xvision-engine/migrations/` reaches 025 (cache & window).
  Per `team/MANIFEST.md` the next available number is **030**; V2D
  reserves 029 (see "Migration coordination" below).

## Raw items → tracks

| Raw item | Track | Lane | Notes |
|---|---|---|---|
| Write the canonical cortex memory integration plan referenced from the install-customizer spec (was a dangling reference; written 2026-05-21 alongside this intake) | `v2d-cortex-memory-plan` | foundation | **Lands first as a doc-only PR.** Everything downstream cites it. Plan lives at `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`. |
| New `xvision-memory` crate: SQLite-backed store keyed by namespace (`global` / `agent:<agent_id>`), top-k cosine retrieval, Embedder trait with OpenAI / Voyage adapters, `MemoryStore` open/upsert/query/forget API | `v2d-xvision-memory-crate` | foundation | Independent of the rest of the workspace; can ship as a standalone PR with its own unit tests. Migration **not** required — the crate manages its own SQLite file (`~/.xvn/memory.db`) so the engine migration registry stays clean. |
| `agent_slots.memory_mode` column + `AgentSlot.memory_mode: MemoryMode { off, global, agent_scoped }` field; store roundtrip; engine migration 029; **slot-level toggle, agent-id-scoped namespace** so multiple slots in one Agent share the agent-scoped bucket but each independently opts in | `v2d-agent-memory-mode` | foundation | Claims migration **029**. Depends on `v2d-xvision-memory-crate` for the `MemoryMode` enum import. |
| Dispatcher wiring: `execute_slot` recalls top-k for `memory_mode != off`, prepends to `system_prompt`; post-dispatch recorder writes `(context_digest, decision_text)` to the slot's namespace | `v2d-dispatcher-wiring` | foundation | Depends on the memory crate + slot field. Single edit site in `crates/xvision-engine/src/agent/execute.rs`; no per-provider changes. |
| UI: Memory selector in `AgentForm.tsx` next to provider / model / temperature; ts-rs regen pulls `MemoryMode` into `frontend/web/src/api/types.gen/` | `v2d-memory-mode-ui` | leaf | Depends on the slot field being persisted. Stays in `frontend/web/src/components/agent/` — no global UI surface. |
| Eval review surface: emit `memory_recall` and `memory_write` events on `events.jsonl`; render in the eval-review run detail UI as a small "Memory" panel per cycle | `v2d-eval-review-memory-surface` | leaf | Depends on the dispatcher wiring. Independent of the UI track. Without this, eval review audits an incomplete picture (board-v2 V2D notes). |

## Dependency graph

```
v2d-cortex-memory-plan  (doc only; lands first as standalone PR)
    │
    ├─→ v2d-xvision-memory-crate  (new crate; standalone tests)
    │       │
    │       └─→ v2d-agent-memory-mode  (claims migration 029)
    │               │
    │               └─→ v2d-dispatcher-wiring
    │                       │
    │                       ├─→ v2d-memory-mode-ui  (parallel-safe)
    │                       │
    │                       └─→ v2d-eval-review-memory-surface  (parallel-safe)
```

Conductor recommendation: ship `v2d-cortex-memory-plan` as a doc-only
PR first so the next four tracks can cite a stable plan reference.
Then ship `v2d-xvision-memory-crate` and `v2d-agent-memory-mode`
serially (the slot field imports `MemoryMode` from the crate).
`v2d-dispatcher-wiring` consumes both. The two leaves are independent
once the dispatcher lands.

## Locked decisions

These resolve at the intake layer so per-track contracts don't
re-derive them:

| # | Decision |
|---|---|
| 1 | **In-process, not sidecar, for v1.** The install-customizer spec names a `cortex-http` sidecar; that's the v2 shape. v1 ships the `xvision-memory` crate as an in-process SQLite-backed store. The HTTP boundary is a needless container hop until F28 plugin architecture lands and there's a reason to expose memory across multiple xvn processes on the same host. The crate's public API is HTTP-shaped (request/response value types, no shared in-process state) so the sidecar boundary can be inserted later without changing the dispatcher seam. |
| 2 | **SQLite-backed, embedded.** Storage lives at `~/.xvn/memory.db` (or `$XVN_MEMORY_DB` override). Uses the workspace's existing `sqlx::SqlitePool` machinery so the operator's backup story is unchanged ("everything xvn writes is under `~/.xvn`"). No new database engine. Vector search is cosine top-k computed in Rust over a `f32` blob column; with a v1 store ceiling of ~10k memories per namespace this is sub-millisecond on any host that runs the rest of xvn. |
| 3 | **Embedder is the slot's own provider when possible.** The slot already has a `provider` + `model` config; the embedder adapter picks the matching embedding model id from a small static table (`openai → text-embedding-3-small`, `voyage → voyage-3-lite`, etc.). For providers that don't expose embeddings (Anthropic), the embedder falls back to a configured `default_embedder` from `~/.xvn/memory.toml` (default: OpenAI). The slot dispatcher does **not** invent embedder credentials — if no embedder is configured and the slot's provider can't embed, memory is silently disabled for that slot and a `memory_disabled_no_embedder` event is emitted (no popup; the eval review surface explains why). |
| 4 | **Slot-level toggle, agent-id-scoped namespace.** `AgentSlot.memory_mode: MemoryMode { off, global, agent_scoped }`. Multiple slots inside one `Agent` that all set `agent_scoped` share the same `agent:<agent_id>` bucket — agent identity is the namespace, not slot identity. Slot identity is a free-text label and would not survive a slot rename; agent id is a ULID and is stable. |
| 5 | **Auto-recall + auto-write, no tool surface.** v1 does not expose a `memory_recall` / `memory_write` tool to the model. Recall is automatic (top-k retrieval prepended to `system_prompt` as a bracketed `<prior_observations>` block); write is automatic post-decision (`(context_digest, decision_text)` summarized to a short memory item by a thin recorder, not a model call). Tool-driven memory is v1.1. Rationale: simpler dispatcher seam, no new tool-loop iterations charged to the operator's budget, no risk of the model emitting unbounded `memory_write` storms. |
| 6 | **Default off.** `MemoryMode::default() == Off`. New slots get `memory_mode = off`; existing rows pre-migration-026 read back as `off`. No agent acquires memory implicitly. Default-off matches the install-customizer's "v1 preset = memory off" decision. |
| 7 | **Forget is explicit + operator-driven; no TTL in v1.** UI exposes a "Clear memory" button per namespace (global, per-agent); CLI exposes `xvn memory forget --namespace global` and `xvn memory forget --agent <agent_id>`. No time decay, no LRU eviction. v1 store ceiling is ~10k items per namespace; the operator owns the eviction policy until V3 autooptimizer proves a need for automatic decay. |
| 8 | **Privacy: 127.0.0.1 only, no external creds for storage.** The SQLite file lives in `~/.xvn/memory.db`, mode 0600. Embedder calls go through the existing provider client (so they inherit the provider's API key handling); no new credential surface is introduced. Operator can audit `memory.db` with `sqlite3` directly. |
| 9 | **No cargo feature gate.** The `xvision-memory` crate is a regular workspace member, always compiled. The install-customizer spec contemplated a `memory` cargo feature, but Decision 1 (in-process v1) makes that gate cost more than it saves — the crate is small, has no transitive heavy deps, and operators turn memory off by leaving every slot at `memory_mode = off`. F28 plugin architecture can introduce the feature gate later if it materially shrinks the binary; v1 does not need it. |
| 10 | **Eval review surface emits two new event kinds.** `memory_recall { namespace, k, items: [{id, text, score}] }` and `memory_write { namespace, id, text_preview }`. Both land on the existing `events.jsonl` per-run sink. No new SQLite table. The eval-review UI gains a "Memory" panel per cycle that filters these two kinds out of the event stream. |

## Migration coordination

- `v2d-agent-memory-mode` claims engine migration **029**
  (`029_agent_slot_memory_mode.sql` adds a `memory_mode TEXT NOT NULL
  DEFAULT 'off'` column to `agent_slots`). The contract author updates
  `team/MANIFEST.md` in the same commit.
- The memory crate does **not** touch the engine migration registry —
  it ships its own SQLite schema in `crates/xvision-memory/migrations/`
  and opens its own pool. The store layer migrates the file lazily on
  first open.

## Out of this intake

- **`cortex-http` sidecar.** Decision 1. Revisit when F28 plugin
  architecture lands.
- **Cross-host memory sharing.** Multiple xvn deployments on the same
  operator account writing to a shared store. Needs auth + conflict
  resolution; out of scope until V3 autooptimizer operates across
  hosts.
- **Tool-driven memory** (`memory_recall` / `memory_write` exposed as
  agent tools). Decision 5. v1.1 if operator data shows auto-recall
  missing relevant items.
- **Cross-namespace retrieval** (a slot in `agent_scoped` mode also
  surfacing `global` matches). Decision 4 keeps namespaces strict in
  v1; a "memory_blend" toggle is v1.1 if operators ask.
- **Embedding model swap migration.** v1 stores embeddings against the
  slot's embedder at write time. If the operator later changes
  embedders, old vectors are not re-embedded — the cosine math still
  works inside one namespace as long as a single embedder is used per
  namespace. A re-embed CLI is a v1.1 chore.
- **Memory-aware findings in eval review.** Surfacing "this decision
  was likely driven by memory item M3" is post-V2D — needs the eval
  review wave to consume the new `memory_recall` events and correlate
  them to decision text. Open as a follow-up after `v2d-eval-review-
  memory-surface` lands.
- **TTL / time decay / LRU eviction.** Decision 7. Operator-driven
  forget is enough until V3 autooptimizer gates a real eviction
  story.
- **Memory across strategy versions.** When a strategy is republished
  with a new hash, does its agent-scoped memory carry forward?
  Decision 4 says yes (agent_id is the namespace, strategy hash is
  not). Revisit if marketplace publishing wants a clean-slate
  guarantee.

## Verification (when a track lands)

Each decomposed track should, at minimum:

- **`v2d-cortex-memory-plan`:** doc-only PR; verification is "the plan
  exists at the path the install-customizer spec already references"
  and the plan covers each Decision 1–10 above with implementation
  detail. No code changes; no test changes.
- **`v2d-xvision-memory-crate`:** unit tests at
  `crates/xvision-memory/tests/` covering (a) open / upsert / query /
  forget roundtrips against an in-memory pool, (b) cosine score
  ordering matches a hand-computed expected, (c) namespace isolation
  (a write to `agent:A` is invisible to a query against `agent:B` and
  against `global`), (d) `MemoryStore::open` lazy-creates the file +
  schema on first open. No engine touch; `cargo test -p
  xvision-memory` is the gate.
- **`v2d-agent-memory-mode`:** unit tests at
  `crates/xvision-engine/src/agents/store.rs` for `memory_mode`
  roundtrip including the default-off behavior on pre-029 rows.
  Migration 029 has a matching `_down.sql`. `cargo test -p
  xvision-engine` is the gate; `bash scripts/board-lint.sh` for the
  manifest update.
- **`v2d-dispatcher-wiring`:** integration test at
  `crates/xvision-engine/tests/agent_memory_dispatch.rs` covering (a)
  `memory_mode = off` produces zero recall / write events and an
  unchanged `system_prompt`, (b) `memory_mode = agent_scoped` with a
  pre-seeded memory store prepends the `<prior_observations>` block
  in the expected shape, (c) the post-dispatch recorder writes one
  memory item with the expected namespace, (d)
  `memory_disabled_no_embedder` is emitted when the slot's provider
  can't embed and no default embedder is configured.
- **`v2d-memory-mode-ui`:** Vitest at
  `frontend/web/src/components/agent/agents.test.tsx` covering (a)
  the Memory selector renders three options matching `MemoryMode`,
  (b) saving with `agent_scoped` round-trips the value through the
  agent PUT, (c) the ts-rs surface compiles
  (`pnpm --dir frontend/web typecheck`).
- **`v2d-eval-review-memory-surface`:** integration test that an eval
  run with `memory_mode = agent_scoped` produces the two new event
  kinds on `events.jsonl` in the expected shape; the eval-review run
  detail UI renders a Memory panel (component test). Vitest covers
  the panel; the event emission is covered by an extension of the
  `v2d-dispatcher-wiring` integration test.

Across all tracks:

- Type-check the dashboard if a ts-rs surface changed:
  `pnpm --dir frontend/web typecheck && pnpm --dir frontend/web test --run`.
- Run `bash scripts/board-lint.sh` before pushing a contract edit.
- Do **not** run `cargo` on remote / deploy hosts (per `CLAUDE.md`).

## Open questions for the conductor

These resolve at decomposition, not in this intake:

1. **Should `v2d-dispatcher-wiring` and `v2d-eval-review-memory-surface`
   be a single contract?** They share the event-emission seam. Merging
   shrinks the contract count from 5 to 4 but couples the eval-review
   UI changes to the dispatcher PR; keeping them separate lets the
   dispatcher land + soak before any UI surface depends on the new
   event kinds. **Recommend keep separate** — the dispatcher is a
   Rust-only change and the eval-review surface is the riskier
   frontend-changing track.
2. **Default embedder choice for `memory.toml`.** OpenAI
   `text-embedding-3-small` ($0.02 / 1M tokens, 1536-dim) vs Voyage
   `voyage-3-lite` ($0.02 / 1M tokens, 1024-dim). Both are cheap.
   OpenAI is the more common provider in operator-configured xvn
   installs; Voyage is the higher-quality option on retrieval
   benchmarks. **Recommend OpenAI default**, Voyage as a one-line
   config switch.
3. **Top-k default and ceiling.** Decision 5 prepends top-k matches
   to `system_prompt`. **Recommend k = 5 default, ceiling 20**, both
   operator-configurable per slot in v1.1. v1 ships the defaults
   hardcoded.
4. **Recorder content shape.** The post-dispatch recorder writes
   "what" exactly? Options: (a) `decision_text` verbatim — simplest,
   loses context; (b) a recorder-LLM summary of `(context, decision)`
   — costs an extra small model call per cycle; (c) a deterministic
   template `"At <cycle_id>, with bars <bars_hash>, decided <action>"`
   — cheap, low-signal. **Recommend (a)** for v1; revisit if recall
   quality is poor. Cheaper than (b), more useful than (c).
5. **Cycle-level vs run-level write.** Does the recorder write once
   per cycle or once per run? **Recommend per-cycle** so the
   autooptimizer can replay cycle-by-cycle judgments; that's the
   whole point of having memory at all. Per-run is too coarse.

## Related artifacts

- `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`
  — implementation plan written alongside this intake. Per-track
  contracts cite the plan's phase numbers.
- `docs/superpowers/specs/2026-05-11-install-customizer-design.md` §9
  — the prior cortex / memory plugin contract. v1 honors the privacy
  + manifest design choices (Decision 8); defers the sidecar (Decision
  1).
- `crates/xvision-engine/src/agents/model.rs` — the AgentSlot struct
  `v2d-agent-memory-mode` extends.
- `crates/xvision-engine/src/agent/execute.rs` — the dispatcher seam
  `v2d-dispatcher-wiring` modifies.
- `frontend/web/src/components/agent/AgentForm.tsx` — the UI surface
  `v2d-memory-mode-ui` modifies.
- `team/MANIFEST.md` — migration 029 reservation lands here in the
  same commit as the migration file.
