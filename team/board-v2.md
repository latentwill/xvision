# xvision V2 board

> Roadmap and active contracts for V2A → V2C. Source plan:
> `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`.
>
> Same rules as the main board (`team/board.md`): one line per active track,
> each linking to a `team/contracts/<slug>.md`. Conductor-owned.
>
> Last updated: 2026-05-17.

## Active — V2A (onboarding & docs)

- [v2a-driver-tour](contracts/v2a-driver-tour.md) — leaf · ready · independent
- [v2a-in-app-docs](contracts/v2a-in-app-docs.md) — leaf · ready · independent

`v2a-example-artifacts` merged via #205 on 2026-05-17; archived under
`team/archive/2026-05-17-v2a/`. The remaining two leaves are independent —
safe to claim in parallel.

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

### V2D — agent memory (new phase; enables V3 autoresearcher)

| # | Item | Source |
|---|---|---|
| 15 | Rust cortex memory + per-agent memory toggle (off / global / agent-specific) | New — see "V2D notes" below |

V2D is a prerequisite for the V3 autoresearcher: a mutator/judge loop without
persistent memory keeps re-discovering the same lessons. Land before V3 unless
the autoresearcher track is explicitly scoped as stateless v1.

### V3 — autoresearcher

| # | Item | Source |
|---|---|---|
| 11 | Autoresearcher mutation / eval / judge loop | autoresearcher plans |
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

## Wave intake

- V2A intake: `team/intake/2026-05-16-eval-review-and-v2a.md` (V2A items 1–3 decomposed).
- V2B/V2C/V2D/V3/V4: no intake yet.

## Closeout

When all V2A contracts merge, the conductor:

1. Archives V2A contracts to `team/archive/<date>-v2a/contracts/`.
2. Updates this file to reflect the next active phase (V2B operability,
   V2D memory, or both in parallel — conductor decides at intake time
   based on V3 autoresearcher readiness).
3. Opens the next phase's intake doc and decomposes its items into contracts.

## See also

- Main board (`team/board.md`) for non-V2 active work and eval-review wave.
