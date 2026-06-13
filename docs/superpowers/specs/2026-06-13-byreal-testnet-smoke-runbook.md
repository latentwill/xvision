# Byreal testnet smoke — runbook (bead xvision-ym9v.9)

The one open acceptance gate for byreal: place a real **testnet** order end-to-end
and confirm an `ExecutionReceipt` with `venue=byreal` + a real `venue_order_id`,
then close it. Needs operator creds, so it can't be agent-run unattended.

## Prerequisites (operator provides)

1. A funded **Hyperliquid testnet** account.
2. Its **agent / API wallet key** (trading-only, cannot withdraw) — NOT the
   master key. (Hyperliquid: approve an API wallet, export its key.)
3. The perps CLI reachable: `npx -y @byreal-io/byreal-perps-cli@latest --version`
   (or `npm i -g @byreal-io/byreal-perps-cli`).

## Setup (creds via env or, once PR #996 + runtime-injection land, Settings)

```bash
export BYREAL_PRIVATE_KEY=$(op read 'op://Personal/xvision-byreal-testnet/agent-key')
export BYREAL_NETWORK=testnet          # REQUIRED — live-eval byreal is testnet-gated
# export BYREAL_ACCOUNT=...            # optional
# export BYREAL_LEVERAGE=2             # optional (PR #993)
```

## Guarded smoke (read → tiny order → verify → close)

```bash
# 0. Connectivity / account (read-only)
npx -y @byreal-io/byreal-perps-cli@latest -o json account info
npx -y @byreal-io/byreal-perps-cli@latest -o json signal detail BTC

# 1. Tiny entry through xvision's CLI venue (mainnet path also via --venue byreal;
#    here testnet via BYREAL_NETWORK). Use the SMALLEST size that clears min-notional.
xvn fire-trade --venue byreal --asset BTC --side buy --size-bps 25

# 2. Verify: receipt has venue=byreal + a real venue_order_id; position shows.
xvn portfolio --venue byreal

# 3. Close out.
xvn close-position --venue byreal BTC
```

## Acceptance + bookkeeping

- Capture the entry receipt (`venue=byreal`, non-empty `venue_order_id`) and the
  close receipt as evidence.
- If brackets are exercised (PR #989), confirm the CLI received `--tp`/`--sl`
  (the entry placed protective legs) — check the order on the venue.
- Close bead **xvision-ym9v.9** with the captured receipts; the byreal epic
  (`xvision-ym9v`) then reaches 9/9.

## Notes

- This validates the **real CLI JSON output schema** too — the only part the
  unit tests can't (they ground command construction; see
  `2026-06-13-byreal-perps-cli-grounding.md`). If the output shape differs from
  the `OrderData`/`PositionData`/`AccountData` structs in `byreal.rs`, fix those
  structs (this is the expected place for a real-CLI mismatch to surface).
- Run on testnet ONLY for this gate. Mainnet byreal ordering is the same CLI
  verbs without `BYREAL_NETWORK=testnet`.
