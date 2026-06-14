# xvision

**Non-custodial AI trading agents.** xvision runs LLM-driven trading strategies
against your own broker account, with explicit scope enforcement so xvision
itself never holds your funds. An overnight autoresearcher mutates and
evaluates new strategy variants automatically.

> ⚠️ **This is alpha software. Use at your own risk.** xvision executes real
> trades against real money on whatever broker account you connect. The
> non-custodial design means xvision can't drain your account, but a buggy
> strategy or risk-engine misconfiguration absolutely can lose money. Read the
> safety section below before connecting a non-trivial balance.

## What it does

- Runs trading strategies as LLM-driven decision pipelines (briefing → trader →
  risk gate → execution).
- Holds an Orderly trading-only Ed25519 key per user that can place orders but
  cannot withdraw, transfer, or mint.
- Enforces per-strategy hard-cap × dynamic-quota budgets via a race-free
  reservation pattern; no strategy can exceed its cap even under burst load.
- Logs every order's full lifecycle (emit → risk → simulate → sign → submit →
  fill → close) to an append-only audit log; positions can be reconstructed
  from the log alone.
- Runs an overnight autoresearcher that mutates seed strategies, evaluates
  variants on held-out backtests, and seals survivors as immutable lineage
  artifacts.

## What it does NOT do

- Custody trading capital. You fund your own Orderly account; xvision only
  holds the authority to place trades against it.
- Process withdrawals or transfers. The Orderly trading key is scoped to
  trading only; the broker layer enforces this independently.
- Run unsupervised on production capital without operator oversight. The
  current design assumes a single operator monitoring the system.

## For Agents

If you are an external or embedded agent using this repo, start here:

1. Read `MANUAL.md` for operator commands and environment assumptions.
2. Read `FOLLOWUPS.md` for active engineering tracks and deferred work.
3. If you are running inside Claude Code rooted in this repo, load `.claude/skills/xvision-cli/SKILL.md` for operator/usage tasks or `.claude/skills/xvision-dev/SKILL.md` when editing the codebase. See `.claude/skills/README.md` for the full skill map.
4. For exact CLI usage, run `xvn --help` and read `.claude/skills/xvision-cli/references/cli.md`.
5. For live-node remote control, use the Tailscale-served dashboard node (`xvn.tail2bb69.ts.net` or `xvnej.tail2bb69.ts.net`) rather than assuming arbitrary SSH access.
6. For a shell-free remote CLI helper, use `scripts/xvn-remote.py`.
7. For inline strategy filters, use `docs/operator/filter-dsl-catalog.md`
   for the exact indicator/operator DSL accepted by `xvn strategy set-filter`.
8. Before launching an agent-backed eval, follow the safe path: provider
   readiness (`doctor`, `provider list`, `provider check`, `provider models`)
   → `strategy diagnostics` → `eval validate` → `eval run`.
9. Use precise execution labels: **Filter-gated agent** is the default
   filtered LLM path, **Rules-only mechanical** is intentional no-agent
   deterministic execution, and **Agent-direct** is legacy/discouraged
   model-without-filter execution.

Hard deployment rules for agents:

1. Never run `cargo` on server/deploy hosts.
2. Never do production image builds on server/deploy hosts.
3. Build deploy images on a build/control host, then ship or pull the runtime image.
4. Prefer `scripts/deploy-image.sh --push user@host` for cost-sensitive dev
   deploys that should skip GHCR and GitHub Actions.
5. Use GHCR via `.github/workflows/docker.yml` (`workflow_dispatch`) when you
   need a registry-backed, reproducible image shared across servers.

## Quickstart (for first users)

This walks through a local backtest path with no live orders.

```bash
# 1. Clone and build
git clone https://github.com/latentwill/xvision
cd xvision
cargo build --release

# 2. Initialize xvision config/state
./target/release/xvn init

# 3. Check provider config/readiness
./target/release/xvn doctor --json
./target/release/xvn provider list
./target/release/xvn provider check --name <provider>
./target/release/xvn provider models --name <provider>

# 4. Configure a strategy from a template
./target/release/xvn strategy templates
./target/release/xvn strategy templates --json
STRATEGY_ID=$(./target/release/xvn strategy create --template mean_reversion --name my-first-agent)

# 5. Diagnose and validate before launching evals
./target/release/xvn strategy diagnostics "$STRATEGY_ID" --json
./target/release/xvn eval scenarios
./target/release/xvn eval validate --strategy "$STRATEGY_ID" --scenario crypto-bull-q1-2025 --mode backtest
./target/release/xvn eval run --strategy "$STRATEGY_ID" --scenario crypto-bull-q1-2025 --mode backtest

# 6. Inspect stored runs
./target/release/xvn eval list
```

Or pull the Docker image — see `docker/README.md` for the full mount/env-var
reference:

```bash
docker pull ghcr.io/latentwill/xvision:latest
docker run --rm \
  -e XVN_AUTOMIGRATE=1 \
  -v xvision-data:/data \
  --env-file .env \
  ghcr.io/latentwill/xvision:latest \
  doctor
```

## Web dashboard

`xvn` also ships a single-binary web dashboard. The Vite-built SPA in
`frontend/web/` is baked into the binary at compile time (via `rust-embed`),
so `xvn dashboard serve` boots a full UI with no separate frontend process.

```bash
# locally, after cargo build
xvn dashboard serve --bind 127.0.0.1:8788
# open http://localhost:8788

# in the docker image (the published `:latest` defaults to this)
docker run --rm -p 8788:8788 -e XVN_AUTOMIGRATE=1 \
  -v xvision-data:/data \
  ghcr.io/latentwill/xvision:latest
```

V1 routes: `/` Dashboard, `/setup` Wizard, `/strategies`, `/strategies/:id`,
`/eval-runs`, `/eval-runs/:id`, `/eval-runs/compare`, `/charts/compare`,
`/settings/*`.
`/authoring/:id` remains as a compatibility alias for old inspector links.
See `frontend/README.md` for the full route table and `frontend/DESIGN.md` for
the design synthesis.

> Building from source? `frontend/web/` is a pnpm workspace and must be built
> (`cd frontend/web && pnpm install && pnpm build`) before `cargo build` if
> you want the SPA embedded. The image published from `Dockerfile.deploy`
> does this automatically.

## Remote CLI over Tailscale

Live-node command execution is exposed through the dashboard's typed remote CLI
job API, not arbitrary SSH access. The API accepts a typed argv array only —
no shell, no caller-controlled cwd or env.

See **[`crates/xvision-dashboard/wiki/remote-cli.md`](crates/xvision-dashboard/wiki/remote-cli.md)**
for the full canonical reference: endpoint table, request/response fields,
polling vs SSE, allowlist policy, and safe-to-surface command examples.

Quick summary:

- Use `scripts/xvn-remote.py exec ...` for a shell-free helper (create/poll/output in one call).
- Use `POST /api/cli/jobs` with a typed argv array for direct API access.
- Long-running jobs can be polled through `GET /api/cli/jobs/:id` and output
  retrieved via `GET /api/cli/jobs/:id/output`.
- SSE progress is available at `GET /api/cli/jobs/:id/events`.
- Cancel a running job with `DELETE /api/cli/jobs/:id` (preferred) or
  `POST /api/cli/jobs/:id/cancel` (legacy alias). Both send SIGTERM then
  SIGKILL after a 5-second grace period and are idempotent on terminal jobs.
- Every job is checked against the allowlist policy before spawning. Read-only
  heads (`eval list/show/results/watch/compare`, `strategy show/validate`,
  `scenario show/select`, `doctor`, etc.) are allowed without configuration.
  Mutating/destructive nested paths and server/live-trading heads (`dashboard`,
  `mcp`, `fire-trade`, `close-position`, `init`, etc.) are rejected.
  Bounded eval/experiment/bakeoff jobs require their strict-template flag set
  (`--max-decisions`, `--max-wall-clock`, etc.).

## Safety

xvision assumes a single operator who monitors the system and can intervene.
Current operator commands:

- `xvn portfolio --venue <alpaca|orderly>` — read live portfolio state.
- `xvn close-position --venue <alpaca|orderly> --asset BTC` — close one open position.
- `xvn fire-trade --venue <alpaca|orderly> --side buy --size-bps 100` — manual smoke trade through the venue executor.
- `xvn store stats --db data/store.db` — inspect local flight-recorder state.
- `xvn eval list` and `xvn eval get <run_id>` — inspect eval history.

The non-custodial design closes one failure mode (xvision can't drain you) but
opens others:
- A buggy strategy can lose its hard-cap allocation. Set caps small at first.
- The autoresearcher can produce a variant that overfits the judge. Lineage
  attestations are explicit about which strategies are sealed (auditable) vs
  which are still mutating (use-with-care).
- Cross-margin contagion: if Orderly applies losses across the whole account,
  one strategy's drawdown can trigger another's stop-loss. v1 either uses
  isolated margin (if available) or fails-closed on aggregate utilization > 85%.

## Architecture

- **Operator surfaces:** the React/Vite dashboard, `xvn` CLI, and `xvn-mcp`
  all call the same `xvision-engine::api` layer instead of duplicating
  business logic.
- **Authoring model:** strategies are bundles with manifests, risk config,
  mechanical params, and AgentRefs/PipelineDef composition over workspace
  agents. Legacy fixed slots still parse for compatibility.
- **Eval loop:** scenarios are DB-backed and seeded with canonical rows.
  Backtest mode replays cached bars through `BacktestExecutor`; paper mode
  uses Alpaca broker-surface credentials. Runs, decisions, equity, findings,
  and attestations persist in SQLite.
- **Memory subsystem:** `xvision-memory` provides the current Observation /
  Pattern memory substrate and is planned to become a trading-safety adapter
  over [gambletan/cortex](https://github.com/gambletan/cortex), the MIT-
  licensed persistent memory engine credited in `CREDITS.md`.
- **Dashboard runtime:** `xvision-dashboard` serves the embedded SPA, JSON API,
  wizard/chat SSE, CLI-job SSE, and live run chart streams from one axum
  binary.
- **Optional identity rail:** `xvision-identity` contains draft ERC-8004
  manifest/reputation clients. It is opt-in and not required for the default
  dashboard/eval loop.

## Documentation

- `MANUAL.md` — operator runbook (commands, daily checklist, scale tiers)
- `docs/operator/filter-dsl-catalog.md` — inline strategy filter indicators,
  operators, and examples for chat rail and CLI agents
- `architecture.md` / `architecture-diagram.mermaid` — current system shape
- `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` — active V2-V4 roadmap
- `frontend/README.md` and `frontend/DESIGN.md` — shipped dashboard routes and design notes
- `crates/xvision-dashboard/README.md` — embedded dashboard API notes
- `docs/superpowers/specs/` — design specifications
- `docs/superpowers/plans/` — implementation plans (executable)
- `docs/HACKATHON-1-PAGER.md` — narrative pitch
- `docs/marketing-followups.md` — public-copy follow-ups and external references
- `docker/README.md` — Docker image guide

## License

Apache-2.0. See `LICENSE` if present, or `Cargo.toml` workspace metadata.
