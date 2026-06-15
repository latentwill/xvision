#!/usr/bin/env bash
# Orderly MAINNET round-trip smoke — REAL MONEY.
#
# Mirror of scripts/orderly-testnet-smoke.sh, but against the production
# Orderly gateway (https://api-evm.orderly.org). Loads mainnet creds from
# 1Password (xvision-orderly-mainnet), submits a *tiny* market BUY and closes
# it with a reduce-only SELL — both through the real `xvn fire-trade` →
# OrderlyExecutor path (Ed25519 signing).
#
# Guards (fail-closed):
#   - refuses unless ORDERLY_BASE_URL is a mainnet (non-"testnet") gateway
#   - refuses unless ORDERLY_MAINNET_CONFIRM=1 is set (real-money ack)
#
# Usage:
#   ORDERLY_MAINNET_CONFIRM=1 scripts/orderly-mainnet-smoke.sh [path-to-xvn-binary]
#   ORDERLY_MAINNET_CONFIRM=1 SIZE_BPS=25 ASSET=SOL scripts/orderly-mainnet-smoke.sh
set -euo pipefail

XVN_BIN="${1:-xvn}"
SIZE_BPS="${SIZE_BPS:-25}"   # 0.25% of equity — keep tiny; raise only if below venue min-notional
ASSET="${ASSET:-SOL}"        # SOL: deeper book than BTC for a small market order
DRY_RUN="${DRY_RUN:-0}"      # 1 = signed read only (confirm creds+signing on mainnet, no order, no funds)

export ORDERLY_KEY=$(op read 'op://Olympus/xvision-orderly-mainnet/key')
export ORDERLY_SECRET=$(op read 'op://Olympus/xvision-orderly-mainnet/secret')
export ORDERLY_ACCOUNT_ID=$(op read 'op://Olympus/xvision-orderly-mainnet/account_id')
# base_url is optional in the vault: unset → OrderlyExecutor defaults to mainnet.
export ORDERLY_BASE_URL=$(op read 'op://Olympus/xvision-orderly-mainnet/base_url' 2>/dev/null \
  || echo 'https://api-evm.orderly.org')

# Fail-closed guard #1: never let a stale testnet URL run as "mainnet".
case "$ORDERLY_BASE_URL" in
  *testnet*|*TESTNET*)
    echo "refusing: ORDERLY_BASE_URL looks like a testnet gateway: $ORDERLY_BASE_URL" >&2
    echo "          this is the MAINNET smoke; use orderly-testnet-smoke.sh for testnet." >&2
    exit 1 ;;
esac

# Dry-run pre-flight: a signed READ only — confirms creds + Ed25519 signing
# against the mainnet gateway through the real Rust executor (standard base64),
# moves zero funds, places no order. Run this FIRST on mainnet.
if [ "$DRY_RUN" = "1" ]; then
  echo "== DRY RUN: signed read only (no order) =="
  echo "   gateway: $ORDERLY_BASE_URL"
  "$XVN_BIN" portfolio --venue orderly
  echo
  echo "== dry-run OK: creds + signing confirmed on mainnet =="
  echo "   re-run with DRY_RUN=0 ORDERLY_MAINNET_CONFIRM=1 to fire the round-trip."
  exit 0
fi

# Fail-closed guard #2: real-money acknowledgement required (live round-trip only).
if [ "${ORDERLY_MAINNET_CONFIRM:-}" != "1" ]; then
  echo "refusing: this fires a REAL-MONEY order on Orderly mainnet ($ORDERLY_BASE_URL)." >&2
  echo "          confirm signing first with: DRY_RUN=1 scripts/orderly-mainnet-smoke.sh" >&2
  echo "          then re-run with ORDERLY_MAINNET_CONFIRM=1 to proceed." >&2
  exit 1
fi

echo "############################################################"
echo "## ORDERLY MAINNET SMOKE — REAL MONEY                      ##"
echo "## gateway: $ORDERLY_BASE_URL"
echo "## asset:   $ASSET   size: ${SIZE_BPS} bps of equity        ##"
echo "############################################################"

echo
echo "== portfolio (before) =="
"$XVN_BIN" portfolio --venue orderly

echo
echo "== BUY (open long, ${SIZE_BPS} bps of equity) =="
"$XVN_BIN" fire-trade --venue orderly --side buy --size-bps "$SIZE_BPS" \
  --asset "$ASSET" --summary "orderly mainnet smoke: open long (real-money round-trip)"

echo
echo "== CLOSE (reduce-only, exact venue qty) =="
"$XVN_BIN" close-position --venue orderly --asset "$ASSET"

echo
echo "== portfolio (after) =="
"$XVN_BIN" portfolio --venue orderly

echo
echo "== smoke complete: confirm fill + flat above =="
