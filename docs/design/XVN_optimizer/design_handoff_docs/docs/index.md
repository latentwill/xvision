# xvn — Operator documentation

> Non-custodial AI trading agents that improve themselves.

**Status** · Alpha · Apache-2.0 · v0.9-alpha (commit `a73b18f`) · Updated 2026-05-20.

xvn runs LLM-driven trading strategies against your own broker account, with explicit scope enforcement so xvn never holds your funds. An overnight autoresearcher mutates and evaluates new strategy variants automatically.

## Context for AI agents

- **route**: `/docs/`
- **audience**: operators, first-100 users, embedded agents
- **key terms**: non-custodial · Orderly · Mantle · Alpaca paper · Intern→Trader · risk layer · autoresearcher · ERC-8004 · Δ-Sharpe · flight recorder
- **do not**: run `cargo` on deploy hosts · build prod images on deploy hosts · assume arbitrary SSH on live nodes · treat `xvision-identity` as required (opt-in only)
- **mirrors**: [index.md](index.md) · [sitemap.json](sitemap.json) · [llms.txt](llms.txt)

## At a glance

| | |
|---|---|
| Trading rail | Orderly · Mantle |
| Default eval | Backtest · Alpaca paper |
| Pipeline | Intern → Trader → Risk → Execution |
| Identity | ERC-8004 · opt-in |

## What it does, and does not

| Does | Does *not* |
|---|---|
| Place trades against your Orderly account with a scoped Ed25519 key. | Custody trading capital. You fund Orderly directly. |
| Enforce per-strategy hard-cap × dynamic-quota budgets, race-free. | Process withdrawals or transfers. Trading scope only. |
| Log every order's full lifecycle to an append-only audit log. | Run unsupervised on production capital without operator oversight. |
| Run an overnight autoresearcher that seals survivors as lineage artifacts. | Bridge funds between chains. Pre-position USDC on Mantle yourself. |

## Quickstart · local backtest in five minutes

This walks through the local backtest path. No live orders, no broker credentials.

```bash
# 1 · Clone and build
git clone https://github.com/latentwill/xvision
cd xvision
cargo build --release

# 2 · Initialize xvn config and state
./target/release/xvn migrate

# 3 · Verify provider config
./target/release/xvn doctor --json
./target/release/xvn provider list

# 4 · Configure a strategy from a template
./target/release/xvn strategy templates
STRATEGY_ID=$(./target/release/xvn strategy create \
  --template mean_reversion \
  --name my-first-agent)

# 5 · Launch a backtest against a canonical scenario
./target/release/xvn eval run \
  --strategy "$STRATEGY_ID" \
  --scenario crypto-bull-q1-2025 \
  --mode backtest

# 6 · Inspect stored runs
./target/release/xvn eval list
```

Docker (the published `:latest` defaults to running the dashboard on `:8788`):

```bash
docker run --rm -p 8788:8788 -e XVN_AUTOMIGRATE=1 \
  -v xvision-data:/data \
  ghcr.io/latentwill/xvision:latest
```

## Where to next

- [Operator manual](manual.md) — tiered milestones, paper trading, Orderly onboarding, ERC-8004 mint, dashboard auth, observability.
- [CLI reference](cli.md) — verbatim `xvn --help` plus strategy authoring, eval, A/B compare, providers, remote CLI over Tailscale.
- [Architecture](architecture.md) — four-stage pipeline with two LLM roles, deterministic risk layer, eval framework with pre-committed metrics.

## For agents reading this site

Every page is plain semantic HTML and readable without JavaScript. Every page is mirrored at the same path with `.md`. A machine-readable site map lives at [sitemap.json](sitemap.json); a short routing index at [llms.txt](llms.txt).

Canonical entry points for embedded agents:

1. `MANUAL.md` — operator commands and environment assumptions
2. `FOLLOWUPS.md` — active engineering tracks and deferred work
3. `.claude/skills/xvision-cli/SKILL.md` for operator tasks · `xvision-dev/SKILL.md` when editing the codebase
4. `xvn --help` — typed CLI surface; argv-only, no shell text
5. `xvn.tail2bb69.ts.net` — Tailscale-served dashboard nodes for remote control
6. `scripts/xvn-remote.py` — shell-free remote CLI helper

### Hard deployment rules

1. Never run `cargo` on server/deploy hosts.
2. Never do production image builds on server/deploy hosts.
3. Build deploy images on a build/control host, then ship or pull the runtime image.
4. Prefer `scripts/deploy-image.sh --push user@host` for cost-sensitive dev deploys that should skip GHCR.
5. Use GHCR via `.github/workflows/docker.yml` (`workflow_dispatch`) for registry-backed reproducible images.

## Safety model

**This is alpha software. Use at your own risk.** xvn executes real trades against real money on whatever broker account you connect. The non-custodial design means xvn cannot drain your account, but a buggy strategy or risk-engine misconfiguration absolutely can lose money.

xvn assumes a single operator who monitors the system and can intervene. The non-custodial design closes one failure mode but opens others:

- **Buggy strategies lose hard-cap allocation.** Set caps small at first.
- **Autoresearcher overfits the judge.** Lineage attestations are explicit about which strategies are sealed (auditable) vs which are still mutating (use with care).
- **Cross-margin contagion.** If Orderly applies losses across the whole account, one strategy's drawdown can trigger another's stop-loss. v1 either uses isolated margin (if available) or fails-closed on aggregate utilization > 85%.

Operator commands kept close at hand:

```bash
xvn portfolio        --venue alpaca
xvn close-position   --venue orderly --asset BTC
xvn fire-trade       --venue alpaca  --side buy --size-bps 100
xvn store stats      --db data/store.db
xvn eval list
xvn eval get         <run_id>
```

---

Generated from `README.md`, `MANUAL.md`, `architecture.md`, and `docs/runbook/` in the [xvision repo](https://github.com/latentwill/xvision). Reconciled with commit `a73b18f` on 2026-05-20.
