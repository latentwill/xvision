# xvision

xvision is a *non-custodial* AI trading system.

That means:

- xvision can place trades for you
- xvision does **not** hold your money
- your broker account stays yours
- you are still responsible for the strategies you run

> ⚠️ **Alpha software**
>
> xvision can trade real money. A bad strategy, bad settings, or a broken
> connection can still lose money. Start small and read the safety notes before
> using real funds.

## What xvision does

- Runs trading strategies with AI help.
- Checks each trade against risk rules before sending it.
- Uses an Orderly trading-only key that can trade, but cannot withdraw funds.
- Saves a full log of what happened so you can review trades later.
- Runs overnight research to try new strategy ideas and keep the good ones.

## What xvision does not do

- It does not hold your trading capital.
- It does not withdraw or transfer funds.
- It should not be left unattended on real money without someone watching it.

## Quick start

This is the fast path for trying xvision on Orderly testnet.

```bash
# 1. Clone and build
git clone https://github.com/latentwill/xvision
cd xvision
cargo build --release

# 2. Create or reuse a signing key
# 3. Set up an Orderly testnet account with that key
# 4. Initialize xvision
export CREDENTIAL_SECRET=$(openssl rand -hex 32)
./target/release/xvn migrate

# 5. Check provider settings
./target/release/xvn provider list

# 6. Create a strategy from a template
./target/release/xvn strategy templates
STRATEGY_ID=$(./target/release/xvn strategy new --template mean_reversion --name my-first-agent)

# 7. Run a backtest
./target/release/xvn eval scenarios
./target/release/xvn eval run --strategy "$STRATEGY_ID" --scenario crypto-bull-q1-2025 --mode backtest

# 8. View saved runs
./target/release/xvn eval list
```

You can also use the Docker image:

```bash
docker pull ghcr.io/latentwill/xvision:latest
docker run --rm \
  -e XVN_AUTOMIGRATE=1 \
  -v xvision-data:/data \
  --env-file .env \
  ghcr.io/latentwill/xvision:latest \
  store stats --db /data/store.db
```

## Web dashboard

xvision includes a web dashboard in the main binary.

```bash
# local
xvn dashboard serve --bind 127.0.0.1:8788
# open http://localhost:8788

# docker
docker run --rm -p 8788:8788 -e XVN_AUTOMIGRATE=1 \
  ghcr.io/latentwill/xvision:latest
```

Main routes:

- `/` — dashboard
- `/setup` — setup wizard
- `/strategies` — strategy list
- `/authoring/:id` — strategy editor
- `/eval-runs` — evaluation history
- `/settings/*` — settings pages

For the full route list, see `frontend/README.md`.

## Safety

xvision is built for one operator who watches what it is doing.

Use these commands to check the system:

- `xvn portfolio --venue <alpaca|orderly>` — view live portfolio state
- `xvn close-position --venue <alpaca|orderly> --asset BTC` — close one position
- `xvn fire-trade --venue <alpaca|orderly> --side buy --size-bps 100` — send a manual test trade
- `xvn store stats --db data/store.db` — inspect local state
- `xvn eval list` and `xvn eval show <run_id>` — review past eval runs

Important warnings:

- A bad strategy can still lose money.
- Keep starting caps small.
- The research system can overfit if you trust it too much.
- If your broker uses shared margin, one strategy can affect another.

## Architecture at a glance

- **Trading layer**: trades, risk checks, audit logs, and broker integration
- **Marketplace layer**: separate future scope for fees and delegation
- **Autoresearcher**: creates, tests, and ranks new strategy variants

## For agents

If another agent is using this repo, start with:

1. `MANUAL.md`
2. `FOLLOWUPS.md`
3. `.claude/skills/xvision/SKILL.md` if running in Claude Code
4. `xvn --help`

Deployment rules for agents:

- Do not run `cargo` on server/deploy hosts.
- Do not build production Docker images on server/deploy hosts.
- Use the GitHub Actions deploy workflow for releases.
- Use `scripts/deploy-ghcr.sh` for deploy/build flow.

## Documentation

- `MANUAL.md` — operator runbook
- `docs/superpowers/specs/` — design specs
- `docs/superpowers/plans/` — implementation plans
- `docs/HACKATHON-1-PAGER.md` — short pitch
- `docs/marketing-followups.md` — marketing follow-ups
- `docker/README.md` — Docker guide

## License

Apache-2.0
