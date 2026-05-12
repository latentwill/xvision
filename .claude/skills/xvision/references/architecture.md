# xvision architecture

Crate layout, pipeline stages, slot model. Cross-reference: top-level `architecture.md` in the repo for the canonical diagram.

## Crate layout

| Crate | Role |
|---|---|
| `xvision-core` | Shared types — `StrategyBundle`, `Algorithm` trait, `Cycle`, `Briefing`, `TraderDecision`, `RiskDecision` |
| `xvision-engine` | Engine + API surface (`crates/xvision-engine/src/api/`) — backtest runner, settings, search, bundle store |
| `xvision-cli` | `xvn` binary; subcommands under `crates/xvision-cli/src/commands/*.rs` |
| `xvision-dashboard` | axum HTTP server + embedded SPA; routes under `src/routes/*.rs` |
| `xvision-eval` | Eval harness — A/B compare, baselines (`src/baselines/`), gate logic |
| `xvision-intern` | Intern backends (`OpenAICompatIntern`, `AnthropicIntern`, `AcpxIntern`) |
| `xvision-mcp` | MCP tool surface (rsi/sma/ema/macd/bollinger/atr/donchian/fib/health) |
| `xvision-execution` | Venue executors (Alpaca, Orderly) — used by `fire-trade` / `close-position` |
| `xvision-identity` | ERC-8004 IdentityRegistry + ReputationRegistry client (opt-in — `cargo build -p xvision-identity`) |
| Vendored skill catalogs | `.claude/skills/byreal/` + `.claude/skills/mantle/` — git submodules feeding xvision's *own* Stage-1 Intern prompt (Claude Code skills, distinct from the deprecated in-app surface) |

## Pipeline (the four roles)

```
Cycle (snapshot) ──▶ Intern ──▶ Briefing ──▶ Trader ──▶ TraderDecision ──▶ Risk ──▶ RiskDecision ──▶ Executor ──▶ Venue
```

A `StrategyBundle` parameterises each stage as a **slot**: `intern_slot`, `trader_slot`, `risk_slot`, `executor_slot`. A slot carries a prompt, a model_requirement, and an allowed_tools set. Reusable prompt authoring lives on the `Agent` entity (`/agents`, `engine::agents`) — the Plan 2b `xvn skill attach` surface was removed in ADR 0012.

## Intern backends (pick deterministically)

| Backend | When to pick |
|---|---|
| `OpenAICompatIntern` | Backtests — deterministic, cache-pairable per cycle_id. Default. |
| `AnthropicIntern` | Backtests — same property, against Anthropic. |
| `AcpxIntern` | Forward-paper / live only — **agentic, breaks deterministic pairing**. Never for A/B compare. |
| OpenRouter (F24) | Same shape as `OpenAICompatIntern`, routes via openrouter.ai. |

## Eval harness — A/B pairing

The A/B compare lives in `xvision-eval`. **Tier 1 invariant** (`docs/superpowers/specs/`): cache keys pair per `setup_id`/`cycle_id` so treatment and baseline see the same cycles. Don't break this. Baselines live in `crates/xvision-eval/src/baselines/`.

Anti-overfit gate (`xvn gate`): verdict in `{Pass, Borderline, Fail}` based on the pre-committed metric set vs treatment effect size + CI.

## Storage layout (`$XVN_HOME`)

Default `~/.xvn/`, override via `XVN_HOME` env var.

```
$XVN_HOME/
├── xvn.db                # SQLite flight recorder (cycles, briefings, decisions, risk_outcomes, executions, traces)
├── config/default.toml   # Provider configs, intern settings, identity toggles
├── secrets/              # Mode 0600 — brokers.toml (Alpaca), provider api keys
├── strategies/           # <agent_id>.json bundles
└── skills/               # <name>.md xvision-internal markdown skills
```

## Dashboard data flow

axum router (`crates/xvision-dashboard/src/server.rs`) → handlers in `src/routes/*.rs` → `xvision_engine::api::*` functions → SQLite pool. Each handler builds an `ApiContext` via `AppState::api_context()`. Actor is currently `Actor::Cli { user: "dashboard" }` — per-user identity arrives with **F35** dashboard auth.

The SPA (`frontend/web/`) uses TanStack Query against `/api/*`. The `index.html` and `assets/*` are baked into the binary by `rust-embed` reading `crates/xvision-dashboard/static/` at compile time. The `assets/*` route reattaches the `assets/` prefix before `Assets::get` (otherwise the embed key doesn't match — see PR #75).
