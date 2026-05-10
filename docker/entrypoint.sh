#!/usr/bin/env bash
# xvision container entrypoint.
#
# Behavior:
#   - ensures /data exists (it's the canonical mount for store.db, traces, vectors)
#   - if XVN_AUTOMIGRATE=1, runs `xvn store migrate --db /data/store.db` before exec
#   - execs `xvn` with the caller's args; default arg is `--help`
#
# Env vars consumed:
#   XVN_AUTOMIGRATE       if "1", run store migrate before exec (default: 0)
#   XVN_DATA_DIR          override /data (default: /data)
#   XVN_CONFIG_DIR        override /config (default: /config)
#   APCA_API_KEY_ID       Alpaca paper key (passed through to xvn)
#   APCA_API_SECRET_KEY   Alpaca paper secret
#   APCA_API_BASE_URL     defaults to paper-api.alpaca.markets
#   ORDERLY_KEY / ORDERLY_SECRET / ORDERLY_ACCOUNT_ID / ORDERLY_BASE_URL
#   MANTLE_RPC_URL / MANTLE_DEPLOYER_KEY  (only when running the identity image)
set -euo pipefail

DATA_DIR="${XVN_DATA_DIR:-/data}"
CONFIG_DIR="${XVN_CONFIG_DIR:-/config}"

mkdir -p "$DATA_DIR"

if [[ "${XVN_AUTOMIGRATE:-0}" == "1" ]]; then
  echo "[entrypoint] running store migrate against $DATA_DIR/store.db" >&2
  xvn store migrate --db "$DATA_DIR/store.db"
fi

if [[ $# -eq 0 ]]; then
  set -- --help
fi

exec xvn "$@"
