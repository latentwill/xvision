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
