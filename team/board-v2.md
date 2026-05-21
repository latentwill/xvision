# xvision V2 board

> Roadmap and active contracts for V2A → V2C. Source plan:
> `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`.
>
> Same rules as the main board (`team/board.md`): one line per active track,
> each linking to a `team/contracts/<slug>.md`. Conductor-owned.
>
> Last updated: 2026-05-20.

## Active — V2F (strategy authoring & user knowledge)

Decomposed 2026-05-21 from `team/intake/2026-05-20-strategies-folder-and-template-refactor.md`.
Plan: `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`
(resolves the four open intake questions; locks location at
`$XVN_HOME/strategies/`, copy+manifest pre-population, `pdftotext`
+ CSV-summary import, three separate wizard tools).

Six tracks, three waves:

**Wave 1 — independent, parallel-safe (foundation + two leaves):**
- [strategies-folder-surface](contracts/strategies-folder-surface.md) — foundation · ready · gates wave 2 + 3 · new `crates/xvision-engine/src/strategies_folder/` module + `list_strategies_folder` / `read_strategies_file` wizard tools
- [agent-pipeline-template-library-expansion](contracts/agent-pipeline-template-library-expansion.md) — leaf · ready · 4–6 new agent templates added to `agents/templates.rs`
- [wizard-prompt-strategy-folder-and-templates](contracts/wizard-prompt-strategy-folder-and-templates.md) — leaf · ready · refresh `prompts/wizard.md` to describe folder + new tools + expanded library; closes loop on #275

**Wave 2 — after foundation merges, parallel-safe:**
- [strategies-folder-prepopulation](contracts/strategies-folder-prepopulation.md) — leaf · ready · `xvn strategies init` + copy from `docs/strategies/` with provenance manifest
- [strategies-folder-import](contracts/strategies-folder-import.md) — leaf · ready · `xvn strategies import` CLI + dashboard `/strategies-folder` drop-zone + `pdftotext` summaries

**Wave 3 — after prepopulation merges:**
- [strategy-ideas-tool-surface](contracts/strategy-ideas-tool-surface.md) — leaf · ready · `list_strategy_ideas` wizard tool that queries the pre-populated library

## Active — V2A (onboarding & docs)

- [v2a-driver-tour](contracts/v2a-driver-tour.md) — leaf · ready · independent
- [v2a-in-app-docs](contracts/v2a-in-app-docs.md) — leaf · ready · independent

`v2a-example-artifacts` merged via #205 on 2026-05-17; archived under
`team/archive/2026-05-17-v2a/`. The remaining two leaves are independent —
safe to claim in parallel.

## Active — V2D (agent memory)

Decomposed 2026-05-21 from `team/intake/2026-05-21-v2d-agent-memory.md`.
Per-intake choice: a single contract carries the whole wave on one branch
(`task/v2d-agent-memory`) with five internal phases per the plan. The
5-phases-as-one-contract shape was chosen because Phases 1→2→3 are
strictly sequential (compile dependencies) and Phases 4+5 share the
event surface that Phase 3 introduces — splitting the wave into five
contracts would add coordination overhead with no parallelism payoff.

- [v2d-agent-memory](contracts/v2d-agent-memory.md) — foundation · claimed · single-contract wave · claims migration **026**

Implementation plan:
`docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`.

## Active — V2E (eval accuracy & trace surface)

Decomposed 2026-05-20 from `team/intake/2026-05-19-eval-accuracy-and-trace-surface.md`.
**All seven contracts are `status: ready` — no worker has claimed any of
them yet, no worktrees exist.** This is intentional: the conductor is
holding V2E in the dock until the V2A leaves merge so capacity isn't
split between waves.

The intake's optional 9→7 re-pairing is applied:
`eval-cost-model-per-bar-and-volume-share` merges items 19+20 from the
research doc; `eval-candle-integrity-and-manifest` merges items 18+21.
See the V2E notes section below for why and the dependency graph.

- [eval-trace-surface-foundation](contracts/eval-trace-surface-foundation.md) — foundation · ready · **lands first, blocks 3 downstream**
- [eval-candle-integrity-and-manifest](contracts/eval-candle-integrity-and-manifest.md) — foundation · ready · independent (claims migration 024)
- [eval-cost-model-per-bar-and-volume-share](contracts/eval-cost-model-per-bar-and-volume-share.md) — foundation · ready · blocks intra-bar (no migration; per-bar arrays live in bars Parquet)
- [eval-intra-bar-fill-ordering](contracts/eval-intra-bar-fill-ordering.md) — leaf · ready · depends on cost-model + foundation
- [eval-lookahead-bias-prober](contracts/eval-lookahead-bias-prober.md) — leaf · ready · depends on foundation
- [eval-broker-rule-findings](contracts/eval-broker-rule-findings.md) — leaf · ready · independent
- [eval-net-of-inference-cost-metric](contracts/eval-net-of-inference-cost-metric.md) — leaf · ready · depends on foundation + `model-call-cost-usd-population`

Recommended sequencing (per the intake's dependency graph):

1. `eval-trace-surface-foundation` lands first as a solo step — every
   downstream track emits into the trace shape it lands. Resist letting
   item-1 leaves ship in parallel against the foundation; the per-finding
   retrofit cost is higher than the wait.
2. After foundation merges, fan out the remaining six. Three sequenced
   pairs:
   - `eval-cost-model-per-bar-and-volume-share` →
     `eval-intra-bar-fill-ordering` (intra-bar strictly consumes the
     fill-price machinery).
   - `eval-candle-integrity-and-manifest` →
     (no downstream within V2E, but trust-receipt renderer follow-up
     depends on it).
   - `eval-broker-rule-findings`,
     `eval-lookahead-bias-prober`, and
     `eval-net-of-inference-cost-metric` are independent leaves —
     parallel safe.

Migration coordination: foundation claims **023**, candle-integrity
claims **024**; both reserved in `team/MANIFEST.md` 2026-05-20.
`eval-net-of-inference-cost-metric` may claim **025** if it needs a
small `run_metrics_summary.net_return_pct` column — decide at the
contract author's first checkpoint.

`crates/xvision-engine/src/eval/executor/backtest.rs` is now a four-track
multi-owner zone (foundation + cost-model + intra-bar + broker-rule);
see `team/OWNERSHIP.md` for the disjoint-region rule.

## Follow-ups / research needed

- **Capability-first agent model — a.k.a. the agent-role refactor** (new
  — needs spec). The current role-based design still bakes too much
  behavior into `trader` vs `router` naming. Refactor toward explicit
  capabilities that are granted separately from labels:
  - a role label is user-facing / prompt-defined
  - behavior comes from the prompt and attached capabilities, not the
    class name
  - `trade` is a capability, not an identity
  - a router can coordinate specialists without having `trade`
  - a trader-capable agent can be attached to any strategy shape that
    needs it
  - newer users can still start from templates, but templates should be
    capability presets / examples layered on top of the base system

  **Folded in (was an F-11 sub-item, now part of this refactor):** the
  **recorder wireup for the eval path**. Today `tool_calls`, `events`,
  `supervisor_notes`, `approvals`, `sandbox_results`, `checkpoints`,
  `artifacts` are all empty for eval-driven runs because the harness
  side and the eval-executor side maintain parallel emission paths —
  that asymmetry is itself a symptom of the role/capability confusion
  this refactor resolves. The spec should specify a single capability-
  gated recorder pipeline that both harness and eval-executor invoke,
  so the operational tables fill regardless of which surface produced
  the run. Originally filed as the final F-11 bullet of
  `team/intake/2026-05-19-eval-traces-end-to-end-audit.md` (item f);
  lifted here because piecemeal mirroring without the capability model
  creates yet another role-shaped emission layer. **Not deferred** —
  gated on the capability spec, not on indefinite future work.

  **Also folded in (related QA carryover):** the canonical strategy
  template currently ships no trader agent, so `xvn_validate_draft`
  immediately fails for any fresh template (a strategy needs at least
  one agent with a `trader` role per the strategies refactor). The
  fix shape — "default capability set" on every template — is exactly
  the capability-preset concept the refactor is about. Tracked under
  this item rather than as a one-off so the spec resolves both at
  once. Surfaced 2026-05-20 by PR #369 (the
  `validate_draft_succeeds_for_fresh_template` test is currently
  expected to fail on `main` until this lands).

  Output before contract: short design note under
  `docs/superpowers/notes/` covering capability schema, enforcement
  points, the unified recorder contract, the default-capability-set
  on starter templates, and the migration path from role-gated eval
  to capability-gated eval.

- **User-configurable review-agent profile** (raised 2026-05-18 from
  operator QA round 2). The current review/research agent profile
  hardcodes `anthropic` as its provider. `qa-review-agent-provider-config`
  ships a runtime fallback so review still runs on dashboards without
  Anthropic configured, but the longer arc is a Settings → Review
  Agents UI where the operator picks the profile (system prompt,
  provider, model, memory mode) for the review pass. Ties into the
  broader "expanding and evaluating agent types" V2 piece. Output
  before contract: short design note under `docs/superpowers/notes/`
  scoping the Settings surface + which review passes are configurable
  (results review only, or also research / autoresearcher passes).

- **V2 "walk back"** — research + competitor comparison before scoping.
  What does "walking back" a v2 action (decision/order/agent step) look
  like for users, and how do comparable products (trading copilots,
  agent IDEs, eval platforms) expose undo / revert / replay? No contract
  until the research note lands; park here so it doesn't get lost.
  Output: a short doc under `docs/superpowers/notes/` summarizing
  competitor patterns + recommended xvision shape, then conductor
  decides whether it becomes a V2B/V2C contract.
- **Marketplace "all-included" strategy dependencies** (V3, ties V2C
  marketplace flow). A purchased strategy must be immediately runnable
  by the buyer — expose every dependency the seller's agents relied on:
  models (provider + id), MCP servers, tools, skills, broker/wallet
  shape, memory mode. Need to (a) track those deps on the Strategy
  artifact at mint time, (b) surface them in the listing UI so a buyer
  sees the full bill of materials before purchase, (c) define a method
  for guaranteeing the buyer's runtime can satisfy them on first run
  (auto-prompt for missing API keys, install missing MCPs/skills,
  reject unsupported model ids with a clear remediation, etc.).
  Output: design note under `docs/superpowers/notes/` covering dep
  schema + buyer-side install/verify flow; promotes into a contract
  alongside the SLF3 strategy NFT mint work (V2C item 8).

## Not yet decomposed

The conductor decomposes one phase at a time. Items below are roadmap-only;
no contracts exist yet. Do **not** freelance contracts from this list — go
through `team/intake/<date>-<phase>.md` first.

### V2B — security & operability (next intake)

| # | Item | Source |
|---|---|---|
| 4 | Dashboard mutating-route auth boundary | F35 |
| 5 | Remote CLI orphan recovery + audit trail | F37, remote CLI specs |
| 6 | Broker/wallet/testnet kill switch + limits | security + blockchain plans |

### V2C — on-chain identity (after V2B)

| # | Item | Source |
|---|---|---|
| 7 | Mantle Sepolia identity/reputation address deploy | SLF2, ADR 0008 |
| 8 | Strategy NFT mint + readback flow | SLF3 |
| 9 | Testnet marketplace list/buy/sell/delegate flow | marketplace spec |
| 10 | Reputation + validation receipt write/readback | SLF4, SLF5 |

### V2D — agent memory (decomposed 2026-05-21 — see "Active — V2D" above)

| # | Item | Source |
|---|---|---|
| 15 | Rust cortex memory + per-agent memory toggle (off / global / agent-specific) | Decomposed: contract `v2d-agent-memory`, plan `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md` |

V2D is a prerequisite for the V3 autoresearcher: a mutator/judge loop without
persistent memory keeps re-discovering the same lessons. Land before V3 unless
the autoresearcher track is explicitly scoped as stateless v1.

### V2E — eval accuracy & trace surface (new phase; also enables V3 autoresearcher)

| # | Item | Source |
|---|---|---|
| 17 | Trace-surface foundation — schema enrichment, cycle features parquet, determinism receipts, findings ↔ cycle backreference | `docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md` §5 |
| 18 | Candle integrity validator — OHLC sanity, gap detection, timestamp monotonicity, duplicate-bar guard | Research doc §3.1 |
| 19 | Per-bar cost arrays — scenario fees/slippage/spread as per-bar arrays, not flat scenario constants | Research doc §4.2 |
| 20 | Volume-share slippage — zipline-style `price * (1 ± impact * volume_share²)` with 2.5% volume cap | Research doc §4.3 |
| 21 | Pinned canonical fixtures + content-hash receipts + data manifest (feed / adjustment / calendar / timezone) | Research doc §3.2 |
| 22 | Lookahead-bias prober — freqtrade-style two-pass diff | Research doc §3.5 |
| 23 | Broker-rule findings (crypto-first) — emit findings for orders that would be rejected at the live venue | Research doc §4.12 |
| 24 | Adaptive intra-bar fill ordering — NautilusTrader-style `O→H→L→C` / gap-past-trigger logic; minimal `OrderState` enum; maker/taker aggressor classification | Research doc §4.7 + §4.5 (promoted from follow-up via 2026-05-20 intake update) |
| 25 | Net-of-inference-cost profitability metric — `net_return_pct = gross_return_pct − (inference_cost_quote_total / capital_initial)`; new `inference_cost_dominates_return` finding | Operator review of LLM strategy eval results (2026-05-20 intake update) |

V2E is the second prerequisite for V3 autoresearcher (alongside V2D memory).
The autoresearcher's diff harness, failed-decision reservoir, and feature-vector
ML hooks all assume the trace shape from item 17 already exists; building it
once up front avoids retrofitting traces for every emitted finding kind.

The §4.9b live-micro-calibration harness gates **signed marketplace
attestations** (V2C item 10 readback flow needs honest cost-model inputs);
schedule it pre-marketplace. The §4.9 paper-parity calibration is a parity
test only — useful for software regression, not a truth claim for live
execution.

Pre-existing equities-only items (§3.4 corporate-action ledger, §3.6
point-in-time universe, §4.10 funding/borrow accrual, §4.11 market-impact
research) are punted to a separate equities-readiness follow-up; not in V2E.

See "V2E notes" below for the wave's dependency graph and the review-derived
accept/defer table.

### V2F — strategy authoring & user knowledge (new phase; small, runs parallel with V2E)

| # | Item | Source |
|---|---|---|
| 26 | Strategies folder (read-only): per-workspace `<workspace>/.xvn/strategies/` tree with `notes/`, `docs/`, `strategy-files/`, `evals/`, `library/`; agent tool pair `list_strategies_folder` / `read_strategies_file` | `team/intake/2026-05-20-strategies-folder-and-template-refactor.md` track 1 |
| 27 | Strategies folder pre-population from `docs/strategies/` + `xvn strategies init` CLI | Intake track 2 |
| 28 | Expanded agent-pipeline template library (4–8 new templates beyond the current 3 in `crates/xvision-engine/src/agents/templates.rs`) | Intake track 3 |
| 29 | Strategy ideas tool surface for the wizard (`list_strategy_ideas`) | Intake track 4 |
| 30 | Wizard prompt refresh for strategies folder + expanded templates; closes the loop on the template-optional relaxation from #275 | Intake track 5 |
| 31 | User import flow (`xvn strategies import` + dashboard drop-zone; minimal PDF/CSV → `.summary.md` parse) | Intake track 6 |

V2F is a small, mostly-independent phase. It builds on the already-
merged `wizard-strategy-template-optional` (#275) by giving agents
*more references to consult* (expanded templates + a user-curated
knowledge folder) without re-imposing the requirement that was just
removed.

Conductor's call on phase label: V2F as a standalone phase, or folded
into V2D as an additional "agent-facing knowledge surface" item.

Pre-existing alternative placement: this could ride alongside V2D
(memory) since both are agent-facing knowledge surfaces. The
distinction is user-curated (V2F) vs agent-learned (V2D). They don't
share files; safe to run in parallel either way.

### V3 — autoresearcher

| # | Item | Source |
|---|---|---|
| 11 | Autoresearcher mutation / eval / judge loop | autoresearcher plans |
| 11a | **Autoresearcher = cortex memory distillation pass** — reads V2D Observations, proposes/judges/promotes Patterns, retires stale ones. Needs write access to the Patterns tier (`MemoryStore::upsert_pattern` / `demote_pattern`); auto-recorder is INSERT-only on Observations. Each promoted Pattern must carry `training_window_end` (latest bar timestamp across contributing Observations) so the dispatcher's time-window recall filter can exclude Patterns from in-replay scenarios. Editing semantics (create / supersede / retire) must land before the first nightly autoresearcher run that targets a Pattern-consuming agent — otherwise the loop is purely evaluative and nothing accumulates. | `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md` |
| 12 | Autoresearcher dashboard + lineage review | autoresearcher dashboard plan |
| 13 | Final UI/UX pass across dashboard surfaces | design docs, chart plans |
| 16 | Chart aesthetics + customization pass using Lightweight Charts layout/grid/crosshair/series/scale options | F32, [Lightweight Charts customization](https://tradingview.github.io/lightweight-charts/tutorials/customization/intro) |

### V4 — mainnet readiness

| # | Item | Source |
|---|---|---|
| 14 | Contract audit, launch flags, mainnet runbook | ADR 0008, contract specs |

## V2D notes — cortex memory + per-agent toggle

Prior design context (do not re-derive from scratch):

- `docs/superpowers/specs/2026-05-11-install-customizer-design.md` already
  treats memory as a first-class plugin: a `xvision-memory` crate (cargo
  feature `memory`), a `cortex-http` sidecar service contributed via
  `docker-compose.override.yml`, a `memory.toml` config file, off in the
  v1 install preset.
- That spec references `docs/superpowers/plans/2026-05-11-cortex-memory-integration-plan.md` — **plan does not exist**.
  The V2D intake must write that plan first, then layer the per-agent
  toggle requirement on top.

User-stated requirement (2026-05-16) extending the original:

> Memory is a selectable switch per agent. Each agent can choose **global
> memory** (shared across all agents) or **agent-specific memory** (siloed
> to one agent). Default off.

Implications:

- `AgentSlot` (or `Agent`) gains a `memory_mode: MemoryMode` field —
  enum `{ off, global, agent_scoped }`.
- The `cortex-http` sidecar namespaces stored memories by either a single
  `global` key or an `agent:<agent_id>` key, selected at write time from
  the slot's mode.
- Eval dispatcher passes the resolved memory handle into the model call's
  context / system prompt assembly. Read shape is "top-k relevant prior
  exchanges" — the cortex integration plan must specify the embedding +
  retrieval strategy.
- UI: the agent edit window gets a Memory selector next to provider/model.
- Eval review (eval-review-agent-engine, eval-review-api-cli, eval-review-run-detail-ui)
  needs to see whether memory was used and what was injected, otherwise
  reviews are auditing an incomplete picture.

Open questions for intake (do not decide on the board):

- Cortex sidecar HTTP shape: roll our own, or align to mem0 / Honcho /
  mempalace? (User precedent on the `todoworld` project leaned toward
  consuming an existing library; here they explicitly said "rust cortex
  memory" so the bias is Rust-native, but a thin wrapper over an existing
  vector store is still on the table.)
- Persistence: SQLite-backed, embedded? Or external (Qdrant / Postgres /
  in-process)? Affects sidecar vs in-crate decision.
- Forget / TTL semantics: explicit user-driven forget vs time decay.
- Privacy: the install-customizer spec already binds the sidecar to
  127.0.0.1 with no external creds; confirm that survives the per-agent
  toggle.

Intake doc when this opens: `team/intake/<date>-v2d.md`. Expected
decomposition (preliminary, conductor refines on intake):

1. `v2d-cortex-memory-plan` (foundation) — write the missing integration plan.
2. `v2d-xvision-memory-crate` (foundation) — Rust crate + sidecar service.
3. `v2d-agent-memory-mode` (foundation) — AgentSlot field + dispatcher wiring.
4. `v2d-memory-mode-ui` (leaf) — Memory selector in agent edit window.
5. `v2d-eval-review-memory-surface` (leaf) — show memory usage in eval review.

## V2E notes — eval accuracy & trace surface

Source research doc (do not re-derive): `docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md`.
That doc captures the codebase audit, the SOTA reference scan, the
review-derived accept/defer table, and §8.4's suggested execution order.
The intake doc decomposes it into tracks; the research doc is the
"why" reference.

**Dependency shape (intake will formalize):**

- `eval-trace-surface-foundation` is the foundation track. Items 17–25
  all emit into the trace shape it lands. Conductor should resist letting
  later items ship before this — retrofitting is more expensive than
  coordinated up-front schema bump.
- `eval-per-bar-cost-arrays` (19) is a foundation for
  `eval-volume-share-slippage` (20) and
  `eval-intra-bar-fill-ordering` (24). Order matters.
- `eval-candle-integrity-validator` (18),
  `eval-pinned-fixtures-and-manifest` (21),
  `eval-lookahead-bias-prober` (22),
  `eval-broker-rule-findings` (23), and
  `eval-net-of-inference-cost-metric` (25) are independent leaves once
  the trace foundation is in.

**Review-derived decisions baked into the intake** (full table in research
doc §8.2):

- §4.9 paper-fill calibration renamed to paper-parity-only; live-money
  truth is a separate §4.9b harness that gates signed marketplace
  attestations.
- Run-receipt manifest expands to include `feed` / `adjustment` /
  `calendar` / `timezone` / `session_filter`.
- `broker_rule_violation` family of findings shipped crypto-first;
  equity-specific kinds (PDT, extended-hours, margin) are no-op stubs
  until equities reach the marketplace.
- Trust-receipt UX surface deferred to a renderer after the findings
  substrate exists.
- Agent anti-overfitting suite (hidden scenarios, walk-forward + embargo,
  metric stability, leakage guards, simplicity penalty) deferred to the
  marketplace track.

**2026-05-20 intake update — operator review additions:**

- Item 24 (`eval-intra-bar-fill-ordering`) promoted from research doc's
  "follow-up wave" into V2E. Rationale: without intra-bar fill ordering,
  the per-bar cost model in item 19 + volume-share slippage in item 20
  still produces dishonest fills for limit/stop/TP orders, because every
  one of them still fills at next-bar open. Stops and TPs being
  theatrical isn't a follow-up nicety; it's an active honesty problem
  on the strategies already being evaluated. Promoting closes the gap
  and avoids retrofitting trace foundation for `FillBranch` provenance
  later. Also folds in §4.5 (maker/taker aggressor-side fees) since it
  requires the order lifecycle this item introduces.
- Item 25 (`eval-net-of-inference-cost-metric`) added net-new. Driver:
  operator review of LLM strategy eval results
  (`.worktrees/cli-workbench-wave-b/docs/tests/2026-05-19-llm-strategy-eval-notes.md`)
  noted causal v4 variants returning -0.1% to -1% gross across 49–100
  decisions per scenario. Net of inference cost those runs are
  materially worse, and the eval surface reports only gross. Today
  every "profitable" finding in xvision is a half-truth. The trace
  foundation already records `tokens_in` / `tokens_out` / `model_id`;
  the missing piece is a top-line `net_return_pct` and an
  `inference_cost_dominates_return` finding.
- Rejected addition: backtest smoke-test hardening as a standalone
  track. Verification of the new model belongs inside each track's
  contract (intake "Verification" section enumerates per-track
  coverage); hardening tests of a model being replaced is wasted work.

Intake doc when this opens: `team/intake/2026-05-19-eval-accuracy-and-trace-surface.md`.

## Wave intake

- V2A intake: `team/intake/2026-05-16-eval-review-and-v2a.md` (V2A items 1–3 decomposed).
- V2D intake: `team/intake/2026-05-21-v2d-agent-memory.md` (item 15 decomposed into a single-contract wave; plan at `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`).
- V2E intake: `team/intake/2026-05-19-eval-accuracy-and-trace-surface.md` (items 17–25 decomposed; 7 contracts in `team/contracts/eval-*` all `status: ready`).
- V2F intake: `team/intake/2026-05-20-strategies-folder-and-template-refactor.md` (items 26–31, **decomposed 2026-05-21 → six tracks under V2F Active above; plan at `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`**).
- V2B/V2C/V3/V4: no intake yet.

## Closeout

When all V2A contracts merge, the conductor:

1. Archives V2A contracts to `team/archive/<date>-v2a/contracts/`.
2. Updates this file to reflect the next active phase (V2B operability,
   V2D memory, or both in parallel — conductor decides at intake time
   based on V3 autoresearcher readiness).
3. Opens the next phase's intake doc and decomposes its items into contracts.

## See also

- Main board (`team/board.md`) for non-V2 active work and eval-review wave.
