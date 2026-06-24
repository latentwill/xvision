# xvision

**Non-custodial AI trading agents.** xvision runs LLM-driven trading strategies
against your own broker account, with explicit scope enforcement so xvision
itself never holds your funds. An overnight optimizer mutates and
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
- Runs an overnight optimizer that mutates seed strategies, evaluates
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

## Getting started

xvision ships as a single binary (`xvn`) for macOS, Linux, and Windows.
Pick your path:

### Download a pre-built binary (fastest)

Download `xvn` from the [latest release](https://github.com/latentwill/xvision/releases/latest):

| Platform | Asset |
|---|---|
| macOS (Apple Silicon) | `xvn-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `xvn-x86_64-apple-darwin.tar.gz` |
| Linux (x86_64) | `xvn-x86_64-linux-musl.tar.gz` |
| Windows (x86_64) | `xvn-x86_64-windows-msvc.zip` |

Extract and place the binary on your `PATH`. On macOS/Linux:

```bash
tar xzf xvn-aarch64-apple-darwin.tar.gz
sudo mv xvn /usr/local/bin/
xvn init
```

On Windows (PowerShell):

```powershell
Expand-Archive xvn-x86_64-windows-msvc.zip -DestinationPath .
Move-Item xvn.exe C:\Users\$env:USERNAME\AppData\Local\Microsoft\WindowsApps\
xvn init
```

### Run with Docker

```bash
# Create an env file with your credentials
cp .env.example .env
# Edit .env — add at least one LLM provider key

# Pull and run (the image is private — docker login ghcr.io first)
docker pull ghcr.io/latentwill/xvision:0.38.0
docker run --rm \
  -e XVN_AUTOMIGRATE=1 \
  -e XVN_DASHBOARD_TOKEN="$(openssl rand -hex 32)" \
  -v xvision-data:/data \
  --env-file .env \
  -p 8788:8788 \
  ghcr.io/latentwill/xvision:0.38.0
```

Then open **http://localhost:8788?token=YOUR_TOKEN** — the `?token=` query param
bootstraps a session cookie so you never need to pass the token again.

> **`XVN_DASHBOARD_TOKEN`** is required whenever the dashboard binds to a
> non-loopback address (including Docker and Tailscale). Generate it with
> `openssl rand -hex 32`. More detail in
> [the runbook](crates/xvision-dashboard/wiki/runbook.md#dashboard-authentication).

### Build from source

```bash
git clone https://github.com/latentwill/xvision
cd xvision
cargo build --release
./target/release/xvn init
```

### First run (all platforms)

1. **Check everything works:**
   ```bash
   xvn doctor
   ```

2. **Add an LLM provider** in Settings → Providers in the dashboard, or via CLI:
   ```bash
   xvn provider add --name anthropic --kind anthropic --api-key "$ANTHROPIC_API_KEY"
   ```

3. **Create a strategy from a template:**
   ```bash
   xvn strategy templates              # list available templates
   xvn strategy create --template mean_reversion --name my-first
   ```

4. **Run a backtest:**
   ```bash
   xvn strategy diagnostics my-first --json
   xvn eval run --strategy my-first --scenario crypto-bull-q1-2025 --mode backtest
   xvn eval list
   ```

5. **Open the dashboard** (if not already running):
   ```bash
   xvn dashboard serve --bind 127.0.0.1:8788
   # → http://localhost:8788
   ```

   The dashboard is a full SPA baked into the binary — no separate frontend
   process. V1 routes: `/` Dashboard, `/strategies`, `/eval-runs`, `/settings`.
   See `frontend/README.md` for the full route table.

> **Building from source?** `frontend/web/` is a pnpm workspace. Build it
> first (`cd frontend/web && pnpm install && pnpm build`) before `cargo build`
> to embed the SPA. The Docker image does this automatically.

## Remote access over Tailscale

Bind the dashboard to `0.0.0.0` and connect from your Tailscale node:

```bash
export XVN_DASHBOARD_TOKEN="$(openssl rand -hex 32)"
xvn dashboard serve --bind 0.0.0.0:8788
# → https://<tailscale-node>:8788?token=<XVN_DASHBOARD_TOKEN>
```

For CLI commands on a remote node without SSH, use the typed remote CLI API:

```bash
scripts/xvn-remote.py exec -- xvn eval list
```

See **[remote-cli.md](crates/xvision-dashboard/wiki/remote-cli.md)** for the
full endpoint reference, allowlist policy, and safe-to-surface commands.

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
