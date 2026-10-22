#!/usr/bin/env bash
# Run one free-only xvn optimizer owner for 24 hours, recovering dead/error cycles.
set -Eeuo pipefail

XVN_CONTAINER="${XVN_CONTAINER:-xvn-app}"
XVN_CONFIG="${XVN_CONFIG:-/data/config/default.toml}"
OPT_CONFIG="${OPT_CONFIG:-/data/autooptimizer-free-long.toml}"
STRATEGY="${STRATEGY:-01M1P4H0ZJHCAVQ0Q1HKFJGBT0}"
FREE_PROVIDER="opencode-zen"
FREE_MODEL="muse-spark-1.3-contributor-free"
DURATION_SECS="${DURATION_SECS:-86400}"
CYCLE_TIMEOUT="${CYCLE_TIMEOUT:-12h}"
SLEEP_AFTER_SUCCESS="${SLEEP_AFTER_SUCCESS:-30}"
SLEEP_AFTER_ERROR="${SLEEP_AFTER_ERROR:-120}"
LOG="${LOG:-/data/xvn-free-optimizer-24h.log}"
LOCK="${LOCK:-/data/xvn-free-optimizer-24h.lock}"

# The allowlist is intentionally literal: no paid fallback is permitted.
case "$FREE_PROVIDER/$FREE_MODEL" in
  opencode-zen/muse-spark-1.3-contributor-free) ;;
  *) printf '%s\n' 'FATAL: free provider/model guard failed' >&2; exit 64 ;;
esac

exec 9>"$LOCK"
if ! flock -n 9; then
  printf '%s\n' 'Another 24h free optimizer supervisor already owns the lock.' >&2
  exit 75
fi

start=$(date +%s)
deadline=$((start + DURATION_SECS))
child=""

log() { printf '[%s] %s\n' "$(date -u +%FT%TZ)" "$*" | tee -a "$LOG"; }

stop_child() {
  if [[ -n "$child" ]] && kill -0 "$child" 2>/dev/null; then
    log "Stopping active optimizer child $child"
    kill -TERM "$child" 2>/dev/null || true
    wait "$child" 2>/dev/null || true
  fi
}
trap stop_child TERM INT

log "START duration=${DURATION_SECS}s strategy=$STRATEGY provider=$FREE_PROVIDER model=$FREE_MODEL"
log "One optimizer owner; candidate backtests are serialized by xvn. timeout=$CYCLE_TIMEOUT"

while (( $(date +%s) < deadline )); do
  set +e
  cycle_out=$(mktemp /data/xvn-free-optimizer-cycle.XXXXXX)
  timeout --signal=TERM --kill-after=120s "$CYCLE_TIMEOUT" \
    docker exec -e XVN_CONFIG_PATH="$XVN_CONFIG" "$XVN_CONTAINER" xvn optimize run \
      --config "$OPT_CONFIG" \
      --strategy "$STRATEGY" \
      --cycles 1 \
      --sleep-secs 0 \
      --experiments-per-cycle 2 \
      --mutator-provider "$FREE_PROVIDER" \
      --mutator-model "$FREE_MODEL" \
      --judge-provider "$FREE_PROVIDER" \
      --judge-model "$FREE_MODEL" \
      --max-consecutive-errors 3 \
      --max-consecutive-zero-keep 3 \
      --export /data/optimizer-exports >"$cycle_out" 2>&1 &
  child=$!
  wait "$child"
  rc=$?
  child=""
  cat "$cycle_out" >>"$LOG"
  # xvn can return rc=0 for a session containing an internally errored cycle.
  # Treat the persisted/printed error marker as a failed cycle for recovery.
  if grep -Eq 'cycle errored|[0-9]+ errored|status=failed|Model .* not supported' "$cycle_out"; then
    rc=70
    log 'Detected an internally errored optimizer cycle despite rc=0'
  fi
  rm -f "$cycle_out"
  set -e

  if (( rc == 0 )); then
    log "Optimizer cycle exited cleanly; sleeping ${SLEEP_AFTER_SUCCESS}s"
    sleep "$SLEEP_AFTER_SUCCESS"
  else
    log "Optimizer cycle exited rc=$rc; child is confirmed dead, clearing only stale optimizer lock"
    # The foreground child has exited. unlock is therefore safe here; it is
    # never issued while an optimizer process launched by this supervisor lives.
    docker exec -e XVN_CONFIG_PATH="$XVN_CONFIG" "$XVN_CONTAINER" xvn optimize unlock >>"$LOG" 2>&1 || \
      log 'WARN: stale-lock clear failed; next cycle will fail closed'
    sleep "$SLEEP_AFTER_ERROR"
  fi
done

log 'END 24-hour supervisor window reached'
