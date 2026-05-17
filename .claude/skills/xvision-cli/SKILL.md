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

- `ab-compare` — N-arm backtest, emits `BacktestResult` JSON. The headline run.
- `metrics` / `gate` — pre-committed metrics + anti-overfit verdict (treatment vs baseline).
- `strategy` — author / validate / list saved `Strategy` artifacts (`$XVN_HOME/strategies/<id>.json`).
- `dashboard serve` — axum server with the SPA baked in via `rust-embed`. Default bind `0.0.0.0:8788`.
- `provider` — manage registered LLM providers in `$XVN_HOME/config/default.toml`.
- `intern` / `trader` / `risk` — preview prompts or run one pipeline stage in isolation.
- `store` — SQLite flight-recorder (`xvn.db`) migrate / stats.
- `eod` — end-of-day operator report (markdown to stdout).

## High-value examples

- `xvn strategy ls`
- `xvn strategy show <id>`
- `xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest`
- `xvn provider ls`
- `xvn dashboard serve --bind 127.0.0.1:8788`

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
