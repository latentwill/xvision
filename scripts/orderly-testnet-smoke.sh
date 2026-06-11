#!/usr/bin/env bash
# Orderly testnet round-trip smoke (MANUAL.md M6 exit criterion).
#
# Loads testnet creds from 1Password (xvision-orderly-testnet, created by
# scripts/orderly_testnet_onboard.py), then submits a small market BUY on
# PERP_BTC_USDC and closes it with a reduce-only SELL — both through the
# real `xvn fire-trade` → OrderlyExecutor path.
#
# Usage: scripts/orderly-testnet-smoke.sh [path-to-xvn-binary]
set -euo pipefail

XVN_BIN="${1:-xvn}"
SIZE_BPS="${SIZE_BPS:-100}"   # 1% of testnet equity
ASSET="${ASSET:-SOL}"         # SOL: the BTC testnet book is often dead (market orders cancel)

export ORDERLY_KEY=$(op read 'op://Olympus/xvision-orderly-testnet/key')
export ORDERLY_SECRET=$(op read 'op://Olympus/xvision-orderly-testnet/secret')
export ORDERLY_ACCOUNT_ID=$(op read 'op://Olympus/xvision-orderly-testnet/account_id')
export ORDERLY_BASE_URL=$(op read 'op://Olympus/xvision-orderly-testnet/base_url')

case "$ORDERLY_BASE_URL" in
  *testnet*) ;;
  *) echo "refusing: ORDERLY_BASE_URL is not a testnet URL: $ORDERLY_BASE_URL" >&2; exit 1 ;;
esac

echo "== portfolio (before) =="
"$XVN_BIN" portfolio --venue orderly

echo
echo "== BUY (open long, ${SIZE_BPS} bps of equity) =="
"$XVN_BIN" fire-trade --venue orderly --side buy --size-bps "$SIZE_BPS" \
  --asset "$ASSET" --summary "orderly testnet smoke: open long (M6 round-trip)"

echo
echo "== CLOSE (reduce-only, exact venue qty) =="
"$XVN_BIN" close-position --venue orderly --asset "$ASSET"

echo
echo "== portfolio (after) =="
"$XVN_BIN" portfolio --venue orderly
