#!/usr/bin/env bash
# xvision container entrypoint.
#
# Behavior:
#   - ensures /data exists (it's the canonical mount for xvn.db, traces, vectors)
#   - seeds packaged probe fixtures into /data/probes if the image includes any
#   - seeds packaged strategies into $XVN_HOME/strategies without overwriting
#   - if XVN_AUTOMIGRATE=1, runs `xvn init --xvn-home $XVN_HOME` before exec
#   - execs `xvn` with the caller's args; default arg is `--help`
#
# Env vars consumed:
#   XVN_AUTOMIGRATE       if "1", run xvn init before exec (default: 0)
#   XVN_DATA_DIR          override /data (default: /data)
#   XVN_HOME              override xvn runtime home (default: $XVN_DATA_DIR)
#   XVN_CONFIG_DIR        override /config (default: /config)
#   XVN_SEED_PROBES_DIR   override packaged probe seed dir (default: /opt/xvision/data/probes)
#   XVN_SEED_STRATEGIES_DIR override packaged strategy seed dir (default: /strategies)
#   APCA_API_KEY_ID       Alpaca paper key (passed through to xvn)
#   APCA_API_SECRET_KEY   Alpaca paper secret
#   APCA_API_BASE_URL     defaults to paper-api.alpaca.markets
#   ORDERLY_KEY / ORDERLY_SECRET / ORDERLY_ACCOUNT_ID / ORDERLY_BASE_URL
#   MANTLE_RPC_URL / MANTLE_DEPLOYER_KEY  (only when running the identity image)
set -euo pipefail

DATA_DIR="${XVN_DATA_DIR:-/data}"
export XVN_HOME="${XVN_HOME:-$DATA_DIR}"
CONFIG_DIR="${XVN_CONFIG_DIR:-/config}"
SEED_PROBES_DIR="${XVN_SEED_PROBES_DIR:-/opt/xvision/data/probes}"
SEED_STRATEGIES_DIR="${XVN_SEED_STRATEGIES_DIR:-/strategies}"

mkdir -p "$DATA_DIR"
mkdir -p "$XVN_HOME"

# Cortex memory store lives on the writable data volume so it survives
# container recreation (the default $HOME/.xvn path is ephemeral inside the
# container). Default it under $XVN_HOME unless the operator overrides it,
# and make sure the parent dir exists + is owned by the runtime user — a
# prior QA bug had a root-owned dir under the volume causing uid-1000
# writes to fail.
export XVN_MEMORY_DB="${XVN_MEMORY_DB:-$XVN_HOME/memory.db}"
MEMORY_DB_DIR="$(dirname "$XVN_MEMORY_DB")"
mkdir -p "$MEMORY_DB_DIR"
# Best-effort: only attempt the chown when we're root (rootless/uid-1000
# containers can't chown and don't need to — they already own the dir).
if [[ "$(id -u)" == "0" ]]; then
  chown -R "$(id -u):$(id -g)" "$MEMORY_DB_DIR" 2>/dev/null || true
fi

if [[ -d "$SEED_PROBES_DIR" ]]; then
  mkdir -p "$DATA_DIR/probes"
  cp -Rn "$SEED_PROBES_DIR"/. "$DATA_DIR/probes"/
fi

if [[ -d "$SEED_STRATEGIES_DIR" ]]; then
  mkdir -p "$XVN_HOME/strategies"
  cp -Rn "$SEED_STRATEGIES_DIR"/. "$XVN_HOME/strategies"/
fi

# The image bakes a read-only seed config under $CONFIG_DIR (mounted :ro in
# docker-compose). Provider mutations from Settings → Providers must write
# back, so on first boot we copy the seed into a writable location under
# $DATA_DIR and re-point XVN_CONFIG_PATH there. Subsequent boots see the
# already-seeded file and leave it alone.
#
# CONTRACT: Dockerfile.deploy intentionally does NOT pre-set XVN_CONFIG_PATH
# so that docker-exec sessions inherit the writable path via the XVN_HOME-based
# fallback ($XVN_HOME/config/default.toml). This export is the sole setter of
# XVN_CONFIG_PATH in the container environment.
WRITABLE_CONFIG_DIR="$DATA_DIR/config"
WRITABLE_CONFIG_PATH="$WRITABLE_CONFIG_DIR/default.toml"
mkdir -p "$WRITABLE_CONFIG_DIR"
if [[ ! -f "$WRITABLE_CONFIG_PATH" && -f "$CONFIG_DIR/default.toml" ]]; then
  cp "$CONFIG_DIR/default.toml" "$WRITABLE_CONFIG_PATH"
  echo "[entrypoint] seeded $WRITABLE_CONFIG_PATH from $CONFIG_DIR/default.toml" >&2
fi
# Self-heal a config that exists but isn't writable by the runtime user. Volumes
# seeded by an older root-running image left $WRITABLE_CONFIG_PATH owned by root,
# so the non-root `xvision` runtime could read it (Settings → Providers listed)
# but every provider add/edit failed with EACCES on write — surfaced as an opaque
# 500 "internal error". Rewrite the file in place AS the runtime user, preserving
# its content (the operator's providers/default-LLM), so ownership follows the
# process. Best-effort: it works whenever the parent dir is writable; if even the
# dir is unwritable we warn loudly rather than boot into a silently-broken
# provider surface.
if [[ -f "$WRITABLE_CONFIG_PATH" && ! -w "$WRITABLE_CONFIG_PATH" ]]; then
  heal_tmp="$WRITABLE_CONFIG_DIR/.default.toml.heal.$$"
  if cp "$WRITABLE_CONFIG_PATH" "$heal_tmp" 2>/dev/null \
     && rm -f "$WRITABLE_CONFIG_PATH" 2>/dev/null \
     && mv "$heal_tmp" "$WRITABLE_CONFIG_PATH" 2>/dev/null; then
    echo "[entrypoint] healed unwritable config ownership at $WRITABLE_CONFIG_PATH (rewrote in place as $(id -un))" >&2
  else
    rm -f "$heal_tmp" 2>/dev/null || true
    echo "[entrypoint] WARNING: $WRITABLE_CONFIG_PATH is not writable by $(id -un) (uid $(id -u)) and could not be healed (parent dir not writable). Provider add/edit from Settings will fail with a 500 until you: chown -R $(id -u):$(id -g) $WRITABLE_CONFIG_DIR on the host volume." >&2
  fi
fi
export XVN_CONFIG_PATH="$WRITABLE_CONFIG_PATH"

if [[ "${XVN_AUTOMIGRATE:-0}" == "1" ]]; then
  echo "[entrypoint] running xvn init against $XVN_HOME/xvn.db" >&2
  xvn init --xvn-home "$XVN_HOME"
fi

# Kill any stale xvision-agentd Node processes left from a prior crashed
# container run. A stale agentd holds the Alpaca WebSocket slot (free tier
# allows 1 concurrent connection), causing HTTP 406 on the next live run.
# The agentd is spawned fresh by xvn on each live run, so killing old ones
# is always safe.
STALE_PIDS=$(pgrep -f 'xvision-agentd' 2>/dev/null || true)
if [[ -n "$STALE_PIDS" ]]; then
  echo "[entrypoint] killing stale xvision-agentd processes (PIDs: $(echo "$STALE_PIDS" | tr '\n' ' '))" >&2
  pkill -f 'xvision-agentd' 2>/dev/null || true
  # Give TCP connections time to tear down so Alpaca releases the WS slot.
  sleep 2
fi

if [[ $# -eq 0 ]]; then
  set -- --help
fi

exec xvn "$@"
