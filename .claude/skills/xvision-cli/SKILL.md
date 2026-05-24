---
name: xvision
description: Orient an agent USING xvision as a product — running `xvn` CLI verbs, the embedded dashboard, MCP tools, strategy/eval workflows, the live Tailscale nodes, and the remote CLI job surface. USAGE ONLY. If the task is editing crates/**, frontend/web/**, migrations, contracts, or CI/deploy scripts, use `xvision-dev` instead; do not load this skill just because the cwd is the xvision repo.
---

# xvision (operator / usage skill)

> **Building xvision rather than using it?** Use the `xvision-dev` skill.
> This skill is for end-user operation: running `xvn`, driving the dashboard,
> submitting remote-CLI jobs, interpreting pipeline output.

A multistrategy trading-agent backtest harness. Single CLI binary `xvn` + a baked-in axum + Vite SPA dashboard.

Use this skill when the task is about *using* xvision: running or explaining
`xvn`, managing strategies/agents/scenarios/eval runs, operating the dashboard
nodes, interpreting xvision pipeline vocabulary, or following xvision operator
runbooks.

## Where to look first (operator docs)

- `MANUAL.md` — operator-side prerequisites (Alpaca creds, Orderly onboarding, Mantle minting). Tier 2 = forward-paper, Tier 3 = one-time setup.
- `xvn <verb> --help` — source of truth for any CLI flag.
- `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md` — the shipped remote CLI contract for driving live nodes.

For engineering docs (specs/plans/ADRs/FOLLOWUPS, architecture, team board),
switch to `xvision-dev`.

## CLI quick map

`xvn --help` is the source of truth, but the high-traffic verbs:

- `strategy` — author / validate / list / inspect saved `Strategy` artifacts (`$XVN_HOME/strategies/<id>.json`). Atomic mode (`strategy new --prompt`) creates a Strategy + Agent + provider/model binding in one call. `--family / --hypothesis / --target-regime / --avoid-regime` attach a `Hypothesis` to the strategy.
- `strategy set-filter` / `strategy filter-catalog` — install and inspect the deterministic inline Filter DSL. Always consult `xvn strategy filter-catalog --json` before authoring filters from chat rail; it is the canonical machine-readable catalog.
- `scenario` — author scenarios. Includes `select` (read-only comparable set query), `inspect --card` (plain-text card), `classify` (auto-derive regime labels from bars), `set-regime` (operator-authored labels).
- `eval` — `run`, `list`, `show`, `results`, `watch`, `compare` (with `--markdown` table), `batch` (multi-scenario), `attest`, `export` (canonical `EvalRunExport` JSON, q15 §3), `review`, `validate`.
- `experiment` — ledger that groups a research question + strategy + scenarios. `experiment run` orchestrates pick → batch → bind → `result_json` in one shot; pair with `--wait --compare --markdown` for a publishable summary.
- `ab-compare` — N-arm backtest harness; emits `BacktestResult` JSON. Cycles come from `--cycles <json>`; bars come from `--bars <json>` or the SQLite cache via `--from / --to / --granularity`.
- `metrics` / `gate` — pre-committed metrics + anti-overfit verdict (treatment vs baseline).
- `dashboard serve` — axum server with the SPA baked in via `rust-embed`. Default bind `0.0.0.0:8788`.
- `provider` — manage registered LLM providers in `$XVN_HOME/config/default.toml`. `refresh-models` hits `/v1/models`; `models` reads the cached catalog (no network).
- `agent get <id>` — fetch one agent record from the workspace agent library (shape matches the `agents[]` slot in `EvalRunExport`).
- `obs retention` / `obs janitor` — agent-run retention policy + TTL/max-bytes sweep.
- `run inspect <run_id>` — materialize `xvn_run.json` + `xvn_report.md` for a finished agent run from the SQLite ledger.
- `intern` / `trader` / `risk` — preview prompts or run one pipeline stage in isolation.
- `store` — SQLite flight-recorder (`xvn.db`) migrate / stats.
- `eod` — end-of-day operator report (markdown to stdout).
- `doctor` — print effective `$XVN_HOME` / config / db / provider / template targets.
- `bars` — SQLite-cached historical bars: fetch / ls / rm / gc.
- `migrate` — apply pending migrations + seed (or `--dry-run` to report state).
- `example` — seed curated example strategies, scenarios, and tutorial artifacts.

## High-value examples

```bash
xvn strategy ls
xvn strategy show <id>
xvn strategy new --prompt prompts/trader.md \
  --name funding-fader --provider openrouter --model kimi-k2 \
  --role trader --asset ETH/USD --timeframe 1h \
  --family compression-breakout \
  --hypothesis "Post-compression range breakouts persist for 4–8 bars" \
  --target-regime "post-compression trend" --avoid-regime chop --json

xvn scenario classify --all                              # auto-label regimes
xvn scenario set-regime <id> --regime expansion --volatility high --direction up
xvn scenario select --asset ETH/USD --timeframe 60 --count 4

xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval compare <run_id_a> <run_id_b> --markdown --sort sharpe
xvn eval batch --strategy <id> --scenarios sc_a,sc_b,sc_c --wait

xvn strategy filter-catalog --json
xvn strategy set-filter <strategy_id> --from-json filter.json

xvn experiment run \
  --name reg-breakout-eth-q1 \
  --question "Does the breakout edge survive across 1h ETH regimes?" \
  --strategy <id> --assets ETH/USD --timeframe 60 --count 4 \
  --wait --compare --markdown --output reports/exp-2026-05-20.md

xvn provider list
xvn provider refresh-models --name openrouter
xvn dashboard serve --bind 127.0.0.1:8788
```

## Pipeline vocabulary (locked 2026-05-10, terminology rename Option B)

| Concept | Name |
|---|---|
| Per-decision-cycle id | `cycle_id` (NOT `setup_id`) |
| Local strategy id (pre-NFT-mint) | `agent_id` (ULID; becomes NFT token id post-mint) |
| Pipeline-config artifact | `Strategy` |
| Decision producer (eval baseline) | `Algorithm` (NOT `Strategy`) |
| One A/B arm | `arm` / `Box<dyn Algorithm>` |
| Trader output | `TraderDecision` |
| Risk gate verdict | `RiskDecision` (Approved / Modified / Vetoed) |
| Cycles DB table | `cycles` (formerly `setups`) |

**Pipeline roles** (intern → trader → risk → executor) are valid
conventions, not hardcoded slot names. The current shipped CLI initializes
state with `xvn migrate`; interactive setup/onboarding is handled through the
dashboard wizard and operator runbooks.

## Experiment vs strategy vs eval (mental model)

- **Strategy** — the immutable pipeline configuration (one `Strategy.json`,
  one or more `AgentRef`s). Authored via `xvn strategy new`.
- **Eval run** — one execution of a strategy against one scenario.
  Launched via `xvn eval run` (or `eval batch` for N scenarios in one call).
- **Experiment** — a ledger row that groups a research question + a strategy
  + the set of scenarios that question demands. `xvn experiment run`
  orchestrates the whole loop: it picks scenarios (`--scenarios` explicit,
  or `--assets / --timeframe / --regimes / --target-decisions / --same-decisions`
  selector mode), launches an eval batch, binds the run ids back to the
  experiment row, and (with `--wait --compare --markdown`) emits a
  publishable comparison table. `--decision-budget` is metadata only — it
  records operator intent, it does not cap eval execution.

Use `experiment` when the operator's question is the unit of work
("does this strategy survive across these regimes?"); use a bare `eval batch`
when you just need N runs and don't need the ledger row.

## Inline deterministic Filter DSL

For pure indicator gates, prefer `xvn strategy set-filter <strategy_id>
--from-json <path>` over an LLM Filter agent. The authoritative catalog
is `xvn strategy filter-catalog --json`; the human docs are
`docs/operator/filter-dsl-catalog.md` and `/docs?slug=filter-dsl-catalog`.

Current high-value tokens include ADX/DI (`adx_14`, `di_plus_14`,
`di_minus_14`), recent/persistent operators (`above_for_<bars>`,
`crossed_above_<bars>`, `crossed_below_<bars>`), opening range
(`opening_range_high_30`, `opening_range_low_30`), time-of-day volume
(`rvol_tod_20`), and `volume_zscore_20`.

Filters may include optional `fire` metadata:

```json
{
  "fire": {
    "reason": "trend_breakout",
    "priority": 0.85,
    "tags": ["trend", "breakout", "volume"],
    "context": ["close", "opening_range_high_30", "adx_14", "rvol_tod_20"]
  }
}
```

`fire` does not change the boolean gate. It adds a compact trigger
reason/context bundle to traces and trader briefings when the gate is
active.

## MCP tool peers for new CLI verbs

The CLI workbench wave landed six new MCP tools that mirror the new verbs,
so the chat-rail / MCP-driving agent doesn't shell out to `xvn`:

| MCP tool | CLI equivalent |
|---|---|
| `xvn_strategy_create_atomic` | `xvn strategy new --prompt …` (atomic mode) |
| `xvn_strategy_validate_preflight` | `xvn strategy validate <id>` (returns `eval_ready` + warnings/errors) |
| `xvn_eval_batch_run` | `xvn eval batch --strategy <id> --scenarios …` |
| `xvn_eval_compare_report` | `xvn eval compare …` decorated with behavior summary per row |
| `xvn_scenario_inspect_card` | `xvn scenario inspect <id> --card` |
| `xvn_eval_behavior` | on-demand `BehaviorSummary` for a finished run |

These wrap the same engine APIs the CLI calls — no `xvn`-binary shell-out.
Prefer them over `POST /api/cli/jobs` when an MCP client is already attached.

## Live nodes (operator surface)

Two live `xvn` instances behind Tailscale on `extndly-dev`:

- `https://xvn.tail2bb69.ts.net` — personal node
- `https://xvnej.tail2bb69.ts.net` — QA node

Live-node control means the Tailscale-served dashboard + remote-CLI surface,
not generic SSH orchestration. Assume one of those hostnames unless told
otherwise. Deployment mechanics (image build, compose recreate, cert
issuance) live in `xvision-dev` — operator tasks should call the dashboard /
remote CLI, not run `docker compose` directly.

## Remote CLI over Tailscale

This is the Tailscale-served remote CLI surface for typed argv jobs.
For command-style live-node work, prefer the typed remote CLI job API instead of ad hoc SSH or shell assumptions.

- Use `scripts/xvn-remote.py exec ...` for a shell-free helper that submits typed argv.
- Use `POST /api/cli/jobs` with `argv` for direct API access.
- Use `GET /api/cli/jobs/:id` and `GET /api/cli/jobs/:id/output` to reconnect after disconnects.
- Use `GET /api/cli/jobs/:id/events` for SSE progress.
- `xvn-mcp` is separate stdio MCP, not the HTTP remote-control surface.

## Don'ts (operator-facing)

- Don't recommend `AcpxIntern` for backtest pairing — agentic intern breaks deterministic cache pairing per `cycle_id`. Use `OpenAICompatIntern` or `AnthropicIntern` for backtests.
- Don't bind the dashboard wider than loopback outside Tailscale until **F35** (dashboard auth) lands.
- Don't drive the live nodes through ad-hoc SSH or shell text — use the typed remote-CLI job API or `scripts/xvn-remote.py`.
- Don't bypass the `xvn provider` / `xvn strategy` / `xvn eval` surfaces by editing `$XVN_HOME` files directly — the CLI knows the right invariants and audit hooks.

## Deeper references

- [`references/cli.md`](references/cli.md) — full CLI subcommand surface with examples.
- [`references/architecture.md`](references/architecture.md) — pipeline stages and how `xvn` output maps to the engine internals (operator-relevant only).

Engineering-side deployment + crate-level architecture moved to the
`xvision-dev` skill.

---

*Skills owner: whichever track ships a new `xvn` verb or Filter DSL
surface is responsible for updating this file in the same PR. Last
refresh: 2026-05-24 (Filter DSL trigger-context expansion).*
