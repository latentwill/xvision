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

# The image bakes a read-only seed config under $CONFIG_DIR (mounted :ro in
# docker-compose). Provider mutations from Settings → Providers must write
# back, so on first boot we copy the seed into a writable location under
# $DATA_DIR and re-point XVN_CONFIG_PATH there. Subsequent boots see the
# already-seeded file and leave it alone.
WRITABLE_CONFIG_DIR="$DATA_DIR/config"
WRITABLE_CONFIG_PATH="$WRITABLE_CONFIG_DIR/default.toml"
mkdir -p "$WRITABLE_CONFIG_DIR"
if [[ ! -f "$WRITABLE_CONFIG_PATH" && -f "$CONFIG_DIR/default.toml" ]]; then
  cp "$CONFIG_DIR/default.toml" "$WRITABLE_CONFIG_PATH"
  echo "[entrypoint] seeded $WRITABLE_CONFIG_PATH from $CONFIG_DIR/default.toml" >&2
fi
export XVN_CONFIG_PATH="$WRITABLE_CONFIG_PATH"

if [[ "${XVN_AUTOMIGRATE:-0}" == "1" ]]; then
  echo "[entrypoint] running store migrate against $DATA_DIR/store.db" >&2
  xvn store migrate --db "$DATA_DIR/store.db"
fi

if [[ $# -eq 0 ]]; then
  set -- --help
fi

exec xvn "$@"
