---
name: xvision
description: Orient an agent operating xvision as a product through the `xvn` CLI, dashboard, MCP tools, strategy/eval workflows, live Tailscale nodes, or remote CLI job surface. Do not use this skill for ordinary coding tasks just because the workspace repository is xvision.
---

# xvision

A multistrategy trading-agent backtest harness. Single CLI binary `xvn` + a baked-in axum + Vite SPA dashboard.

Use this skill when the task is about using xvision: running or explaining
`xvn`, managing strategies/agents/scenarios/eval runs, operating the dashboard
nodes, interpreting xvision pipeline vocabulary, or following xvision operator
runbooks. Do not load it for generic code edits, dependency bumps, or ordinary
repository navigation unless the user task also needs xvision product/CLI
context.

## Where to look first

Don't grep blindly. The repo has canonical docs — start there:

- `MANUAL.md` — operator-side prerequisites (Alpaca creds, Orderly onboarding, Mantle minting). Tier 2 = forward-paper, Tier 3 = one-time setup.
- `FOLLOWUPS.md` — open engineering work. **F-codes** = shared track. **SLF-codes** = Strategy Loom hackathon track (branch `hackathon/turing`, deadline 2026-06-15).
- `architecture.md` — system shape.
- `docs/superpowers/specs/` and `docs/superpowers/plans/` — design spec + executable plan for each major subsystem. Plan filenames are dated; pick the latest matching the feature you're touching.
- `decisions/` — ADRs. ADR 0010 = the 2026-05-05 marketplace pivot; ADR 0011 = CV substrate moved to xvision-play.
- `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md` — the shipped remote CLI contract over Tailscale.

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

## Build, test, run

```bash
cargo build --workspace
cargo test --workspace
```

Cargo from `~/.cargo/bin`. `xvision-identity` is opt-in — excluded from `default-members`. Build it explicitly: `cargo build -p xvision-identity`.

## Deploy

GHCR image `ghcr.io/latentwill/xvision:latest` is built from `Dockerfile.deploy` (CLI + SPA baked in) by `.github/workflows/docker.yml`. Trigger: tag push `v*.*.*` or `gh workflow run docker.yml --ref main -f dockerfile=Dockerfile.deploy`.

Two live instances on `extndly-dev`: `xvn.tail2bb69.ts.net` (personal) + `xvnej.tail2bb69.ts.net` (QA). Stacks at `/root/deploy/stacks/{xvn,xvnej}/`. Redeploy: `docker compose pull && docker compose up -d --force-recreate`. App shares netns with the tailscale sidecar — if `ts-*` restarts, `--force-recreate` the app too.

Live-node control currently means the Tailscale-served dashboard node, not
generic SSH orchestration. Assume `xvn.tail2bb69.ts.net` or
`xvnej.tail2bb69.ts.net` unless the operator tells you otherwise.

## Remote CLI over Tailscale

This is the Tailscale-served remote CLI surface for typed argv jobs.
For command-style live-node work, prefer the typed remote CLI job API instead of ad hoc SSH or shell assumptions.

- Use `scripts/xvn-remote.py exec ...` for a shell-free helper that submits typed argv.
- Use `POST /api/cli/jobs` with `argv` for direct API access.
- Use `GET /api/cli/jobs/:id` and `GET /api/cli/jobs/:id/output` to reconnect after disconnects.
- Use `GET /api/cli/jobs/:id/events` for SSE progress.
- `xvn-mcp` is separate stdio MCP, not the HTTP remote-control surface.

## Don'ts

- Don't recommend `AcpxIntern` for backtest pairing — agentic intern breaks deterministic cache pairing per `setup_id`/`cycle_id`. Use `OpenAICompatIntern` or `AnthropicIntern` for that.
- Don't mock the DB in integration tests — production migrations need real exercise.
- Don't bind the dashboard wider than loopback outside Tailscale until **F35** (dashboard auth) lands.
- Don't build heavy Rust locally on `extndly-dev` (3.7 GiB RAM — OOMs). Use GHCR.
- Don't push workflow-file changes with the default `gh` auth on `extndly-dev` — it lacks `workflow` scope. Use the classic PAT from 1Password (`Olympus / Github Classic Token (No Admin/Delete)`).

## Deeper references

- [`references/cli.md`](references/cli.md) — full CLI subcommand surface with examples.
- [`references/architecture.md`](references/architecture.md) — crate layout, pipeline stages, slot model.
- [`references/deploy.md`](references/deploy.md) — GHCR workflow, the xvn / xvnej tailscale stacks, common deploy pitfalls.
