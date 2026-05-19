#!/usr/bin/env bash
# Liquidate all positions and cancel all open orders on the Alpaca paper
# account. Use this between paper eval runs so each run starts with the
# full cash balance.
#
# Why this exists: the Alpaca paper account is singleton. A prior run that
# opens a long and never closes it leaves the cash bucket near-empty,
# which starves later runs' sizing math (`cash × risk_pct` falls below
# Alpaca's $10 minimum notional → every order rejected). The in-engine
# fix (paper.rs cash-aware gate) detects this and skips submits, but the
# RIGHT fix is a clean account at run start. This script is the manual
# version of that.
#
# Requirements: 1Password CLI authenticated (`source /root/.op_env`) and
# `curl` + `jq` on PATH. The "Alpaca API Key" item in the Olympus vault
# must have fields `API KEY`, `Secret`, and `endpoint`.
#
# Usage:
#   scripts/alpaca-paper-reset.sh                # close all positions + cancel orders
#   scripts/alpaca-paper-reset.sh --dry-run      # show what would be closed
#   scripts/alpaca-paper-reset.sh --orders-only  # cancel orders, keep positions
#
# Exit codes: 0 on success, non-zero on any failure.

set -euo pipefail

DRY_RUN=0
ORDERS_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)     DRY_RUN=1; shift ;;
    --orders-only) ORDERS_ONLY=1; shift ;;
    -h|--help)
      sed -n '2,/^set -euo pipefail$/p' "$0" | sed 's/^# \?//;/^set/d'
      exit 0 ;;
    *) echo "alpaca-paper-reset: unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "${OP_SERVICE_ACCOUNT_TOKEN:-}" ]]; then
  if [[ -r /root/.op_env ]]; then
    # shellcheck disable=SC1091
    source /root/.op_env
  else
    echo "alpaca-paper-reset: OP_SERVICE_ACCOUNT_TOKEN not set and /root/.op_env not readable" >&2
    exit 1
  fi
fi

APCA_KEY="$(op item get 'Alpaca API Key' --vault Olympus --fields label='API KEY' --reveal)"
APCA_SECRET="$(op item get 'Alpaca API Key' --vault Olympus --fields label='Secret' --reveal)"
ENDPOINT="$(op item get 'Alpaca API Key' --vault Olympus --fields label='endpoint' --reveal)"

hdr_key=(-H "APCA-API-KEY-ID: $APCA_KEY")
hdr_sec=(-H "APCA-API-SECRET-KEY: $APCA_SECRET")

echo "==> Account before"
curl -sf "$ENDPOINT/account" "${hdr_key[@]}" "${hdr_sec[@]}" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print('  cash=\$'+d['cash'], 'buying_power=\$'+d['buying_power'], 'long_market_value=\$'+d['long_market_value'])"

if [[ "$ORDERS_ONLY" -eq 0 ]]; then
  echo "==> Open positions"
  positions_json="$(curl -sf "$ENDPOINT/positions" "${hdr_key[@]}" "${hdr_sec[@]}")"
  echo "$positions_json" | python3 -c "import json,sys; d=json.load(sys.stdin); print(f'  {len(d)} position(s)'); [print(f'    {p[\"symbol\"]} qty={p[\"qty\"]} mv=\${p[\"market_value\"]}') for p in d]"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "==> [dry-run] would DELETE /v2/positions (close all)"
  else
    echo "==> Closing all positions"
    curl -sf -X DELETE "$ENDPOINT/positions" "${hdr_key[@]}" "${hdr_sec[@]}" \
      | python3 -c "import json,sys; d=json.load(sys.stdin); print(f'  submitted {len(d)} close-order(s)')" || true
  fi
fi

echo "==> Open orders"
orders_json="$(curl -sf "$ENDPOINT/orders?status=open" "${hdr_key[@]}" "${hdr_sec[@]}")"
echo "$orders_json" | python3 -c "import json,sys; d=json.load(sys.stdin); print(f'  {len(d)} open order(s)')"

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "==> [dry-run] would DELETE /v2/orders (cancel all)"
else
  echo "==> Canceling all open orders"
  curl -sf -X DELETE "$ENDPOINT/orders" "${hdr_key[@]}" "${hdr_sec[@]}" >/dev/null || true
fi

if [[ "$DRY_RUN" -eq 0 ]]; then
  # Position closes are market orders — give them a moment to fill before
  # reporting the final balance, otherwise the operator sees the pre-fill
  # snapshot and thinks the reset didn't take.
  sleep 5
  echo "==> Account after"
  curl -sf "$ENDPOINT/account" "${hdr_key[@]}" "${hdr_sec[@]}" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); print('  cash=\$'+d['cash'], 'buying_power=\$'+d['buying_power'], 'long_market_value=\$'+d['long_market_value'])"
fi
