# xvision architecture (contributor view)

Crate boundaries, pipeline shape, storage layout. Cross-reference: top-level
`architecture.md` in the repo for the canonical diagram.

## Crate layout

| Crate | Role |
|---|---|
| `xvision-core` | Shared types — `Strategy`, `Algorithm` trait, `Cycle`, `Briefing`, `TraderDecision`, `RiskDecision` |
| `xvision-engine` | Engine + API surface (`crates/xvision-engine/src/api/`), backtest runner, settings, search, bundle store, SQLite migrations |
| `xvision-cli` | `xvn` binary; subcommands under `crates/xvision-cli/src/commands/*.rs`. The `clap` Cli enum lives in `src/lib.rs` — adding a subcommand means **both** a `commands/<name>.rs` module and a `Cli` variant + dispatch arm |
| `xvision-dashboard` | axum HTTP server + embedded SPA; routes under `src/routes/*.rs` |
| `xvision-eval` | Eval harness — A/B compare, baselines (`src/baselines/`), gate logic |
| `xvision-intern` | Intern backends (`OpenAICompatIntern`, `AnthropicIntern`) |
| `xvision-mcp` | stdio MCP tool surface (rsi/sma/ema/macd/bollinger/atr/donchian/fib/health) |
| `xvision-execution` | Venue executors (Alpaca, Orderly) — used by `fire-trade` / `close-position` |
| `xvision-identity` | ERC-8004 IdentityRegistry + ReputationRegistry client (opt-in — `cargo build -p xvision-identity`) |
| `xvision-observability` | Apache-2.0 — schema, redactor v1, blob store, event bus, recorder trait, retention/janitor (Phase A landed via #200 + #204) |
| `xvision-agent-client` | (planned) Cline SDK sidecar adapter — created by step 3 of the Cline migration plan; gates Phase B observability |
| Vendored skill catalogs | `.claude/skills/byreal/` + `.claude/skills/mantle/` (git submodules) feed xvision's own Stage-1 Intern prompt; distinct from the deprecated in-app skills surface (ADR 0012) |

## Pipeline (the four roles)

```
Cycle (snapshot) ──▶ Intern ──▶ Briefing ──▶ Trader ──▶ TraderDecision
                                    │
                                    ▼
                                  Risk ──▶ RiskDecision ──▶ Executor ──▶ Venue
```

A `Strategy` references reusable `Agent` records via `AgentRef { agent_id, role }`.
The role string is free text — pipeline-stage names (intern, trader, risk,
executor) are conventions, not hardcoded slot fields. Prompt authoring lives
on the `Agent` entity (`/agents`, `engine::agents`); the Plan 2b `xvn skill
attach` surface was removed in ADR 0012.

## Intern backends — pick deterministically

| Backend | When |
|---|---|
| `OpenAICompatIntern` | Backtests — deterministic, cache-pairable per `cycle_id`. Default. |
| `AnthropicIntern` | Backtests — same property, against Anthropic. |
| OpenRouter (F24) | Same shape as `OpenAICompatIntern`, routes via openrouter.ai. |

## Eval harness — A/B pairing (tier-1 invariant)

The A/B compare lives in `xvision-eval`. Cache keys pair per `cycle_id` so
treatment and baseline see the same cycles. Don't break this. Baselines under
`crates/xvision-eval/src/baselines/`.

Anti-overfit gate (`xvn gate`): verdict in `{Pass, Borderline, Fail}` from the
pre-committed metric set vs treatment effect size + CI.

## Storage layout (`$XVN_HOME`)

Default `~/.xvn/`, override via `XVN_HOME` env var.

```
$XVN_HOME/
├── xvn.db                # SQLite flight recorder (cycles, briefings, decisions, risk_outcomes, executions, traces, eval_runs, agent_runs, …)
├── config/default.toml   # Provider configs, intern settings, identity toggles, observability toggles
├── secrets/              # Mode 0600 — brokers.toml (Alpaca), provider api keys
├── strategies/           # <agent_id>.json strategy artifacts
└── skills/               # <name>.md xvision-internal markdown skills
```

## Dashboard data flow

axum router (`crates/xvision-dashboard/src/server.rs`) → handlers in
`src/routes/*.rs` → `xvision_engine::api::*` functions → SQLite pool. Each
handler builds an `ApiContext` via `AppState::api_context()`. Actor is
currently `Actor::Cli { user: "dashboard" }` — per-user identity arrives with
**F35** dashboard auth.

The SPA (`frontend/web/`) uses TanStack Query against `/api/*`. The
`index.html` and `assets/*` are baked into the binary by `rust-embed` reading
`crates/xvision-dashboard/static/` at compile time. The `assets/*` route
reattaches the `assets/` prefix before `Assets::get` (otherwise the embed key
doesn't match — see PR #75; don't undo).

## Migration registry

`crates/xvision-engine/migrations/NNN_<topic>.sql` + matching `_down.sql`.
Numbers are reserved in `team/MANIFEST.md`. Latest landed: **018**
(`agent_run_observability`). Coordinate next-number reservations through the
conductor, not by grabbing the next free integer.

## Observability shape (Phase A)

`xvision-observability` exposes:

- A canonical schema (migration 018, 10 tables — `agent_runs`, `spans`, `checkpoints`, `model_calls`, `tool_calls`, `approvals`, `sandbox_results`, `supervisor_notes`, `artifacts`, `events`).
- `RunEventBus` (bounded mpsc, default capacity 4096, per-`run_id` backpressure counter).
- `AgentRunRecorder` trait → `SqliteRecorder` + `NoopRecorder`.
- `Attribute` newtype with `compile_fail` doctests preventing payload strings from reaching an OTel attribute API.
- Redactor v1, content-addressed blob store, observability.toml loader with env-var precedence.
- `xvn obs retention {show,set,clear}` + janitor for TTL and size caps.

Phase B (IPC emission, OTel bridge, export CLI, UI) is gated on the Cline SDK
migration reaching step 3 (`xvision-agent-client` crate exists). PR #199 is
the design draft.
