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

## Quickstart (for first users)

This walks through running xvision against Orderly testnet with no real money.

```bash
# 1. Clone and build
git clone https://github.com/latentwill/xvision
cd xvision
cargo build --release

# 2. Generate an EVM signing key (or use an existing one)
# 3. Set up Orderly testnet account with that key
# 4. Initialize xvision
export CREDENTIAL_SECRET=$(openssl rand -hex 32)
./target/release/xvn setup
# follow prompts to register Orderly account on testnet

# 5. Issue a trading-only key
./target/release/xvn key issue --user op
# Verify: ./target/release/xvn key verify <pubkey>

# 6. Configure a strategy from a template
./target/release/xvn strategy templates
./target/release/xvn strategy create --from buy_and_hold --agent-id my-first-agent

# 7. Set a budget
./target/release/xvn budget set --agent my-first-agent --hard-cap 100

# 8. Run a single trader cycle and inspect the result
./target/release/xvn run --agent my-first-agent --cycle-id $(uuidgen)
./target/release/xvn audit agent --agent my-first-agent --since 1h
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
  store stats --db /data/store.db
```

## Safety

xvision assumes a single operator who monitors the system and can intervene.
Critical operator commands:

- `xvn kill --strategy <id>` — halt one agent, in-flight positions stay open
- `xvn kill --all` — global halt, every dispatcher refuses new orders
- `xvn unhalt --strategy <id>` — resume after halt
- `xvn emergency-close --all` — flatten every position via market orders
- `xvn audit agent --agent <id> --since 1h` — see every decision in the last hour

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

- **Trading rail** (this scope): non-custodial, broker-side scope enforcement,
  off-chain SQLite audit log + reservation ledger.
- **Marketplace rail** (separate scope, Plan 5): on-chain protocol for fees +
  delegation. xvision.io would run this; a self-hosted instance does not need
  it.
- **Autoresearcher** (separate scope, AR-1/AR-2/AR-3): the mutator + judge +
  lineage seal pipeline.

## Documentation

- `MANUAL.md` — operator runbook (commands, daily checklist, scale tiers)
- `docs/superpowers/specs/` — design specifications
- `docs/superpowers/plans/` — implementation plans (executable)
- `docs/HACKATHON-1-PAGER.md` — narrative pitch
- `docker/README.md` — Docker image guide

## License

Apache-2.0. See `LICENSE` if present, or `Cargo.toml` workspace metadata.
