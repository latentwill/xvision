# Memory Distill Ledger Slice

Date: 2026-05-25

Implemented the first repo-native autooptimizer entry point:

- `xvn autooptimizer run` reads a same-namespace Observation cohort from
  the memory store.
- The run requires at least two Observations, preserving the
  anti-one-hot promotion rule.
- The run mints a staged Pattern through the existing
  `promote_observations` path, so `training_window_end` is computed from
  the latest contributing Observation `source_window_end`.
- The run writes an `autooptimizer_runs` ledger row in the memory DB and
  `xvn autooptimizer inspect` reads it back.
- `xvn flywheel status` summarizes the namespace-level loop state:
  Observation count, active/staged/forgotten Pattern counts, total
  autooptimizer runs, and the latest autooptimizer run id.
- `xvn optimize memory-demos` now bridges DSRs x memory at the substrate
  layer: it selects Observation demos, renders a deterministic
  `<memory_demos>` prompt prefix, dry-runs by default, and mints a child
  Agent with `--yes`. This is not yet a MIPRO/GEPA search loop; it is
  the auditable demo-pool + child-agent handoff the optimizer will
  replace internally.
- `xvn optimize memory-demos` now enforces holdout discipline before
  prompt minting. The selected Observation pool is partitioned into
  train/dev/holdout by `--holdout-split` (default `70/15/15`), only the
  train split is rendered into `<memory_demos>`, and the JSON result
  includes per-split ids plus `sha256:` hashes. `--demo-source` records
  `frozen-snapshot`, `fresh-recorder`, or `manual-csv`; `fresh-recorder`
  is marked non-reproducible in the response. `--manual-csv` supplies an
  explicit Observation id list for manual pools.
- Real memory-demo child mints now persist a durable
  `agent_slot_optimizations` lineage row in `xvn.db` via engine
  migration 039. The row records `optimization_id`, target/child agent,
  slot, method, demo source, reproducibility, holdout split, cohort
  query, train/dev/holdout Observation id JSON, per-split hashes, prompt
  prefix length, status, and timestamp. Dry-runs still return
  `optimization_id = null` and remain side-effect-free.
- Memory-demo optimization can now take opt-in Pattern priors with
  repeatable `--prior-pattern <pattern_id>` on the CLI or
  `prior_pattern_ids` through the dashboard API. Priors are restricted
  to non-forgotten active Patterns in the target namespace
  (`promotion_state IS NULL OR promotion_state = 'active'`), rendered
  ahead of demos inside a `<pattern_priors>` prompt block, returned as
  `prior_pattern_ids` plus `pattern_prior_count`, and persisted through
  engine migration 040 in `pattern_optimizations` with `role='prior'`.
- Memory-demo optimization can also opt into recently recalled Pattern
  priors with `--auto-priors --prior-limit N`, MCP
  `auto_prior_patterns`, and the dashboard "Use recalled Pattern
  priors" checkbox. The selector reads persisted `memory_recall` events
  for the same namespace, orders by latest recall timestamp then id,
  keeps manual priors first, skips demo-source Pattern links, and
  revalidates every auto-selected id against the active Pattern
  namespace guard before rendering or persisting it.
- Memory-demo optimization also records candidate Patterns whose
  autooptimizer source Observation cohort overlaps the selected
  train/dev/holdout demo pool. These links are restricted to same-
  namespace, non-forgotten active/staged Patterns and active/staged
  autooptimizer runs, returned as `demo_source_pattern_ids` plus
  `pattern_demo_source_count`, and persisted in `pattern_optimizations`
  with `role='demo_source'`.
- `scripts/audit-memory-demos.sh` wraps `xvn optimize memory-demos
  --json` and verifies that train/dev/holdout sets are disjoint and
  hash-bearing, giving CI and operators a concrete overlap audit entry
  point.
- The dashboard Flywheel panel now exposes Demo Source and Split controls
  for memory-demo child minting, so the web UI is not silently pinned to
  CLI-only defaults. After minting it renders the demo-source Pattern
  count, prior Pattern count, train demo count, and holdout demo count
  from the optimize response.
- Flywheel velocity is now a read surface across CLI, dashboard API, and
  web UI. `xvn flywheel velocity`, `GET /api/flywheel/velocity`, and the
  Flywheel panel report recent Observation capture, Pattern promotion,
  Pattern demotion, autooptimizer run count, optimized child-agent count,
  and average optimizer lineage depth for a namespace lookback window.
  `scripts/export-flywheel-velocity.sh` exports the same JSON-backed
  values as a Markdown evidence report.
- Flywheel lineage is also readable without spelunking SQLite:
  `xvn flywheel lineage`, `GET /api/flywheel/lineage`, the frontend API
  client, and the Flywheel panel expose optimizer rows for the selected
  namespace, including child agent id, train/dev/holdout Observation
  counts, demo-source Pattern ids, and prior Pattern ids.
- `xvn autooptimizer ls`, `xvn autooptimizer promote <run_id>`, and
  `xvn autooptimizer demote <run_id>` now expose the staged Pattern
  lifecycle from the CLI. Promotion flips the produced Pattern to
  `promotion_state='active'`; demotion soft-deletes it with
  `forgotten_at` so the grace-window restore path remains available.
- Staged Patterns created outside autooptimizer now have Pattern-id
  lifecycle controls too: `xvn memory activate <pattern_id>`,
  `xvn memory demote <pattern_id>`, `POST /api/memory/:id/activate`,
  and `POST /api/memory/:id/demote`.
- `xvn memory ls`, `GET /api/memory`, and the frontend memory client
  accept `promotion_state=active|staged`, `include_forgotten`, and
  `forgotten_only` filters so staged and demoted Pattern drill-downs
  are inspectable instead of only counted.
- Memory namespace discovery is explicit across surfaces:
  `xvn memory namespaces`, `GET /api/memory/namespaces`,
  `listMemoryNamespaces()`, and MCP `xvn_memory_namespaces` report live
  total, Observation count, active/staged Pattern counts, forgotten
  count, and latest creation timestamp by namespace.
- `memory undo-forget` now reconciles restored autooptimizer Pattern
  rows back from `autooptimizer_runs.promotion_state='demoted'` to the
  restored Pattern's live lifecycle state.
- `xvn autooptimizer gate <run_id>` and
  `POST /api/autooptimizer/:id/gate` now persist a plan-aligned
  day/holdout numeric gate:
  `parent_day_score`, `child_day_score`, `parent_holdout_score`,
  `child_holdout_score`, `gate_epsilon`, computed `delta_day`,
  computed `delta_holdout`, `gate_verdict`, and `gate_reason`.
  Both deltas must clear epsilon to pass. The previous generic
  `baseline_score`/`candidate_score` gate remains accepted as a
  compatibility fallback.
- The gate records qualitative Finding provenance fields:
  `qualitative_finding_json`, `finding_blinded_metrics`, `judge_model`,
  and `judge_token_cost`, while continuing to expose the earlier
  `finding_text`/`finding_blind` fields for older clients. `xvn
  autooptimizer promote` rejects runs whose gate explicitly failed or has
  not passed.
- Dashboard API routes now expose the same backend surfaces:
  `GET /api/flywheel/status`, `POST /api/autooptimizer/run`,
  `GET /api/autooptimizer`, `GET /api/autooptimizer/:id`,
  `POST /api/autooptimizer/:id/gate`,
  `POST /api/autooptimizer/:id/promote`,
  `POST /api/autooptimizer/:id/demote`, and
  `POST /api/optimize/memory-demos`. The POST routes live in the
  dashboard mutating router so they inherit the existing auth/audit
  middleware; the GET routes are read-only.
- Frontend API wrappers exist in `frontend/web/src/api/flywheel.ts` for
  the same four dashboard routes, with URL/body tests in
  `frontend/web/src/api/flywheel.test.ts`.
- The persistent memory UI now has a Flywheel panel in both
  `/agents/memory` and the per-agent Memory tab. It shows namespace
  status counts, can stage a candidate Pattern through the
  autooptimizer API, and in agent mode can mint a memory-demo child
  agent through the optimize API. It also lists recent autooptimizer
  runs, lets an operator record the day/holdout numeric gate from the
  dashboard, renders gate/Finding state, and lets an operator promote
  or demote the produced Pattern.
- The dedicated `/agents/:id/flywheel` route is now the full-history
  operator page: it requests 25 autooptimizer runs and 20 optimizer
  lineage rows, rendering "AutoOptimizer History" and "Optimization
  History" instead of the compact latest-row summary used inside the
  Memory tab.
  Pattern rows now expose lifecycle filters plus Activate/Demote
  controls for Pattern-id workflows.
- Phase 0 now has a named leakage-regression harness:
  `crates/xvision-memory/tests/leakage_regression.rs` asserts the
  structural Observation/Pattern boundary, temporal replay filtering,
  same-score recall determinism, forgotten-row recall suppression plus
  admin visibility, and Observation provenance requirements.
- `MemoryStore::query` now uses a deterministic tie-breaker
  (score descending, then memory id ascending) so equal-score recall
  sets render in a stable order.
- `crates/xvision-engine/tests/agent_memory_dispatch.rs` includes a
  composite Phase 0 prompt-path probe that runs `execute_slot` twice
  against freshly seeded stores and asserts byte-identical prompts,
  case-law framing, Pattern-only recall, temporal exclusion, staged
  Pattern exclusion, namespace isolation, and memory-before-base-prompt
  ordering.
- `scripts/leakage-regression.sh` is the operator/CI entry point for
  the Phase 0 harness. It runs the memory F+L+T probes plus the engine
  case-law and temporal prompt-path tests.
- MCP now has read-side parity for the memory/flywheel matrix:
  `xvn_memory_list`, `xvn_memory_get`, `xvn_memory_recall`,
  `xvn_flywheel_status`, and `xvn_flywheel_velocity`. The memory
  recall wrapper is deliberately read-only and requires a caller-supplied
  embedding, then delegates to the same store query path that enforces
  Pattern-only, active-only, forgotten-hidden, and temporal
  `scenario_start` filters. The flywheel wrappers call the existing
  engine API status/velocity surfaces so CLI, dashboard, and MCP share
  the same counters.
- Phase 1.5 attribution preflight is started without dependency churn:
  `CREDITS.md`, `LICENSES/gambletan-cortex.txt`, README Architecture
  credit, and first-mention doc attribution are in place. The companion
  evidence note is
  `docs/superpowers/evidence/2026-05-24-cortex-flywheels/cortex-adoption/attribution.md`.
- Operator skills from the surface matrix now exist under
  `.claude/skills/xvision/`: `memory-ops`, `autooptimizer-ops`, and
  `flywheel-ops`. The skills give agents a runbook for F+L+T-safe memory
  operations, offline Pattern distillation, numeric gate/Finding evidence,
  flywheel velocity/lineage checks, MCP read tools, and script-based
  regression probes.
- MCP autooptimizer read parity is in place with
  `xvn_autooptimizer_list`, `xvn_autooptimizer_inspect`, and
  `xvn_autooptimizer_findings`. These are read-only wrappers over the
  existing run ledger APIs and expose numeric gate/Finding provenance to
  chat-rail/MCP clients without adding a write tool.
- MCP flywheel read parity now includes `xvn_flywheel_lineage`, so chat
  rail / MCP clients can inspect optimization lineage rows, Pattern links,
  and durable train/dev/holdout hashes without shelling out to the CLI.
- MCP now exposes the existing safe optimizer bridge as
  `xvn_optimize_memory_demos`: dry-run by default, with explicit
  `apply=true` required to mint a child agent and persist lineage. The
  MCP fixture proves dry-run hashes, minted child id, demo-source Pattern
  links, prior Pattern links, and lineage hash proof.
- `scripts/export-pattern-lineage.sh` exports a Markdown lineage report
  from `xvn autooptimizer inspect --json` plus the produced Pattern row
  from `xvn memory show --json`, covering the surface-matrix
  pattern-lineage export script.
- `scripts/smoke-autooptimizer-distill.sh` is a one-command smoke harness
  for phase-entry proof: it runs `xvn autooptimizer run --json`, reads the
  same run back through `xvn autooptimizer inspect --json`, then captures
  `xvn flywheel status --json` for the same namespace or agent.
- Durable DSRs holdout proof is now visible after the optimize response is
  gone: `xvn flywheel lineage`, `/api/flywheel/lineage`, and the dashboard
  Latest Lineage panel all expose the persisted train/dev/holdout hashes
  from `agent_slot_optimizations`, with the holdout hash shown in the UI.
- `scripts/audit-memory-demos.sh` now emits the exact train/dev/holdout
  Observation id sets alongside their hashes and accepts `--output`, so
  the proof can be stored as a durable artifact instead of a transient
  terminal check.
- A dedicated frontend route `/agents/:id/flywheel` now reuses the
  existing agent-scoped Flywheel panel without mounting the full Memory
  surface. It preserves the same API scoping as the Memory tab and
  mints memory-demo children against the route agent id.
- Memory-demo optimizations now have a persisted dev/holdout gate on
  `agent_slot_optimizations` via engine migration 041. `xvn optimize
  memory-demos-gate`, `POST /api/optimize/memory-demos/:id/gate`,
  `gateOptimization()`, and the Flywheel lineage UI all write/read the
  same fields: metric names, parent/child dev scores, parent/child
  holdout scores, epsilon, computed deltas, verdict, reason, and
  timestamp. The verdict passes only when both dev and holdout deltas
  clear epsilon.
- Human-readable `xvn flywheel lineage` now prints gate verdict, dev
  delta, holdout delta, reason, and gate timestamp when present, so
  operators do not need `--json` to see whether the optimizer handoff
  passed holdout discipline.
- `scripts/audit-memory-demo-gate.sh` records an optimizer gate through
  `xvn optimize memory-demos-gate`, reads the matching row back through
  `xvn flywheel lineage --json`, verifies verdict/deltas/epsilon are
  identical, and emits one JSON proof that also carries the persisted
  train/dev/holdout hashes.
- Engine migration 041 now has an explicit down migration dropping the
  gate columns and verdict index, giving the schema-change proof a real
  rollback artifact instead of only a compatibility statement.
- `execute_slot` no longer synthesizes Observation source windows from
  wall clock time. If memory is active and either `source_window_start`
  or `source_window_end` is missing, the dispatcher skips the Observation
  write, emits `memory_write_missing_source_window`, and leaves the
  decision path otherwise successful. Positive write tests now pass
  explicit market windows, preserving the invariant that future Pattern
  `training_window_end` derives from contributing bar data only.

Verification commands:

```text
cargo test -p xvision-engine --lib api::autooptimizer::tests
TMPDIR=/Users/edkennedy/Code/xvision/.tmp cargo test -p xvision-engine --lib api::memory::tests
TMPDIR=/Users/edkennedy/Code/xvision/.tmp cargo test -p xvision-engine --lib api::flywheel::tests
TMPDIR=/Users/edkennedy/Code/xvision/.tmp cargo test -p xvision-engine --lib api::optimize::tests
cargo test -p xvision-engine --lib api::flywheel::tests
cargo test -p xvision-engine api::optimize
TMPDIR=/Users/edkennedy/Code/xvision/.tmp cargo test -p xvision-engine --test api_context api_context_open_creates_db_and_runs_migrations
TMPDIR=/Users/edkennedy/Code/xvision/.tmp cargo test -p xvision-engine --features ts-export --lib api::flywheel::tests
TMPDIR=/Users/edkennedy/Code/xvision/.tmp cargo test -p xvision-engine --features ts-export --lib api::optimize::tests
cargo test -p xvision-cli --test autooptimizer_cli
TMPDIR=/Users/edkennedy/Code/xvision/.tmp cargo test -p xvision-cli --test memory_cli
cargo test -p xvision-cli --test memory_cli namespaces_json_summarizes_memory_scopes
TMPDIR=/Users/edkennedy/Code/xvision/.tmp cargo test -p xvision-cli --test optimize_cli
cargo test -p xvision-cli --test optimize_cli
cargo test -p xvision-dashboard --test flywheel_routes
cd frontend/web && npm test -- flywheel.test.ts
cd frontend/web && npm test -- flywheel.test.ts MemoryTab.test.tsx
cd frontend/web && npm test -- MemoryTab.test.tsx agents-flywheel.test.tsx flywheel.test.ts
cd frontend/web && npm test -- MemoryPage.test.tsx MemoryTab.test.tsx flywheel.test.ts
cd frontend/web && npm test -- flywheel.test.ts MemoryPage.test.tsx MemoryTab.test.tsx
cd frontend/web && npm test -- MemoryTab.test.tsx flywheel.test.ts
cd frontend/web && npm test -- MemoryPage.test.tsx
cd frontend/web && npm test -- memory.test.ts MemoryPage.test.tsx MemoryTab.test.tsx
cd frontend/web && npm test -- memory.test.ts
cd frontend/web && npm run typecheck
cd frontend/web && npm test -- agents-flywheel.test.tsx MemoryTab.test.tsx MemoryPage.test.tsx
TMPDIR=/Users/edkennedy/Code/xvision/.tmp cargo test -p xvision-memory
cargo test -p xvision-memory --test leakage_regression
cargo test -p xvision-memory --test store
cargo test -p xvision-engine --test agent_memory_dispatch phase0_leakage_regression_harness_flt_prompt_is_deterministic
cargo test -p xvision-engine --test agent_memory_dispatch execute_slot_skips_observation_write_without_source_window
cargo test -p xvision-engine --test agent_memory_dispatch execute_slot_writes_final_decision_into_namespace
cargo test -p xvision-engine --test agent_memory_dispatch pipeline_threads_memory_recorder_to_execute_slot
cargo test -p xvision-engine --test agent_memory_dispatch execute_slot_emits_memory_recall_event_with_decision_id
bash scripts/leakage-regression.sh
cargo test -p xvision-mcp mcp_
cargo test -p xvision-mcp mcp_memory_read_tools_enforce_recall_filters
cargo test -p xvision-mcp mcp_flywheel_lineage_returns_optimizer_hash_proof
cargo test -p xvision-mcp
cd frontend/web && npm test -- agents-flywheel.test.tsx routes-code-splitting.test.ts
bash -n scripts/export-pattern-lineage.sh
bash -n scripts/smoke-autooptimizer-distill.sh
test -f CREDITS.md
test -f LICENSES/gambletan-cortex.txt
rg -n "gambletan/cortex" CREDITS.md README.md docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md
test -f .claude/skills/xvision/memory-ops/SKILL.md
test -f .claude/skills/xvision/autooptimizer-ops/SKILL.md
test -f .claude/skills/xvision/flywheel-ops/SKILL.md
cargo fmt --check
git diff --check
bash -n scripts/leakage-regression.sh
bash -n scripts/audit-memory-demos.sh
bash -n scripts/audit-memory-demo-gate.sh
bash -n scripts/export-flywheel-velocity.sh
```

Broad `cargo test -p xvision-engine --lib api::` currently exercises
this slice successfully but still fails in unrelated
`api::settings::danger::*` reset-workspace tests due to an existing
SQLite fixture issue: `no such table: main.eval_runs_old_live_migration`.

Known remaining gaps:

- Pattern proposal is still operator-provided via `--pattern-text`; no
  LLM proposer is wired yet.
- Numeric gate and blind Finding are still deterministic/operator-
  supplied ledger fields. Provider-backed metric execution and the
  metrics-blind LLM judge call are not implemented yet.
- DSRs / `dspy-rs` optimizer integration is not present in this repo.
  `xvn optimize memory-demos` is intentionally a deterministic
  split-aware compile/mint bridge, not a metric-search optimizer.
- Bakeoff execution from `xvn optimize memory-demos` is not wired yet;
  the existing `model bakeoff` surface remains the comparison runner.
  The dev/holdout sets are selected and hashed, and manual/API gate
  scores can now be persisted on the optimization row, but the scores
  are not computed automatically by an optimizer loop yet.
