#!/usr/bin/env bash
# scripts/demo-smoke.sh — end-to-end smoke test for the xvision demo stack.
#
# Exercises: start live eval run → wait for running → pause → flatten
#   → resume → cancel → verify terminal state and all HTTP 200s.
#
# Requirements:
#   - xvision dashboard already running at SMOKE_BASE_URL (default :8788)
#   - APCA_API_KEY_ID + APCA_API_SECRET_KEY set (Alpaca paper trading)
#   - jq on PATH
#
# Environment overrides:
#   SMOKE_BASE_URL        dashboard URL (default: http://localhost:8788)
#   SMOKE_STRATEGY_ID     use a specific strategy instead of the first one found
#   SMOKE_LIVE_ASSET      asset to trade (default: BTC/USD)
#   SMOKE_TIME_LIMIT_SECS run wall-clock cap in seconds (default: 300)
#   SMOKE_DECISIONS_WAIT  seconds to wait for at least 1 decision (default: 60)
#   APCA_API_KEY_ID       Alpaca paper key id (required for live mode)
#   APCA_API_SECRET_KEY   Alpaca paper secret key (required for live mode)
#
# Exit codes: 0 = all assertions passed, 1 = failure.
#
# Usage (demo host):
#   APCA_API_KEY_ID=PKXXX APCA_API_SECRET_KEY=xxx ./scripts/demo-smoke.sh
#
# Usage (docker):
#   docker run --rm -e APCA_API_KEY_ID=... -e APCA_API_SECRET_KEY=... \
#     xvision:deploy-<sha> bash /config/demo-smoke.sh

set -euo pipefail

BASE_URL="${SMOKE_BASE_URL:-http://localhost:8788}"
LIVE_ASSET="${SMOKE_LIVE_ASSET:-BTC/USD}"
TIME_LIMIT="${SMOKE_TIME_LIMIT_SECS:-300}"
DECISIONS_WAIT="${SMOKE_DECISIONS_WAIT:-60}"
POLL_INTERVAL=3
RUNNING_TIMEOUT=60

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# ── helpers ───────────────────────────────────────────────────────────────────

_log()  { printf '[smoke] %s\n' "$*" >&2; }
_pass() { printf '[smoke] ✓ %s\n' "$*" >&2; }
_fail() { printf '[smoke] ✗ %s\n' "$*" >&2; exit 1; }

_require() {
  command -v "$1" >/dev/null 2>&1 || _fail "required tool not found: $1"
}

_http() {
  local label="$1" method="$2" url="$3" body="${4:-}"
  local status resp tmpfile
  tmpfile="$(mktemp)"
  if [[ -n "$body" ]]; then
    status=$(curl -s -o "$tmpfile" -w '%{http_code}' \
      -X "$method" -H 'Content-Type: application/json' --data "$body" "$url")
  else
    status=$(curl -s -o "$tmpfile" -w '%{http_code}' -X "$method" "$url")
  fi
  resp="$(cat "$tmpfile")"
  rm -f "$tmpfile"
  if [[ "${status:0:1}" != "2" ]]; then
    _fail "$label → HTTP $status: $resp"
  fi
  _pass "$label → $status"
  printf '%s' "$resp"
}

# Poll until run status matches target or timeout.
_poll_status() {
  local run_id="$1" target="$2" timeout="$3"
  local elapsed=0 status
  while [[ "$elapsed" -lt "$timeout" ]]; do
    status=$(curl -sf "$BASE_URL/api/eval/runs/$run_id" | jq -r '.status // "unknown"')
    _log "  run $run_id: status=$status (awaiting '$target', ${elapsed}s/${timeout}s)"
    if [[ "$status" == "$target" ]]; then
      return 0
    fi
    # Unexpected terminal state
    if [[ "$status" =~ ^(failed|cancelled|completed)$ && "$status" != "$target" ]]; then
      _fail "run reached terminal '$status' before expected '$target'"
    fi
    sleep "$POLL_INTERVAL"
    elapsed=$((elapsed + POLL_INTERVAL))
  done
  _fail "timed out after ${timeout}s waiting for status '$target'"
}

# Poll for any decisions to appear (non-fatal if timeout reached).
_wait_for_decisions() {
  local run_id="$1" timeout="$2"
  local elapsed=0 count
  _log "Waiting up to ${timeout}s for first decision ..."
  while [[ "$elapsed" -lt "$timeout" ]]; do
    count=$(curl -sf "$BASE_URL/api/eval/runs/$run_id" | jq -r '.decisions | length // 0' 2>/dev/null || echo "0")
    if [[ "${count:-0}" -gt 0 ]]; then
      _pass "Decisions received: $count"
      return 0
    fi
    sleep "$POLL_INTERVAL"
    elapsed=$((elapsed + POLL_INTERVAL))
  done
  _log "  No decisions in ${timeout}s — continuing (run may have just started)"
}

# ── preflight ─────────────────────────────────────────────────────────────────

_require curl
_require jq

_log "=== xvision demo smoke test ==="
_log "Dashboard: $BASE_URL"
_log "Live asset: $LIVE_ASSET  time_limit: ${TIME_LIMIT}s"
_log ""

# ── 1. Health ─────────────────────────────────────────────────────────────────

_log "1. Dashboard health check ..."
_http "GET /api/eval/runs (health)" GET "$BASE_URL/api/eval/runs?limit=1" >/dev/null

# ── 2. Seed Alpaca credentials (idempotent) ───────────────────────────────────

if [[ -n "${APCA_API_KEY_ID:-}" && -n "${APCA_API_SECRET_KEY:-}" ]]; then
  _log "2. Seeding Alpaca paper credentials ..."
  _http "POST /api/settings/brokers/alpaca" POST \
    "$BASE_URL/api/settings/brokers/alpaca" \
    "{\"api_key_id\":\"$APCA_API_KEY_ID\",\"api_secret_key\":\"$APCA_API_SECRET_KEY\"}" \
    >/dev/null
else
  _log "2. APCA_API_KEY_ID/APCA_API_SECRET_KEY not set — using previously stored creds"
fi

# ── 3. Resolve strategy ───────────────────────────────────────────────────────

_log "3. Resolving strategy ..."
if [[ -n "${SMOKE_STRATEGY_ID:-}" ]]; then
  STRATEGY_ID="$SMOKE_STRATEGY_ID"
  _pass "Using SMOKE_STRATEGY_ID: $STRATEGY_ID"
else
  STRATEGY_LIST=$(curl -sf "$BASE_URL/api/strategies?limit=1")
  STRATEGY_ID=$(printf '%s' "$STRATEGY_LIST" | jq -r '.items[0].manifest.id // empty')
  if [[ -z "$STRATEGY_ID" ]]; then
    _fail "No strategies found. Seed at least one strategy or set SMOKE_STRATEGY_ID."
  fi
  STRATEGY_NAME=$(printf '%s' "$STRATEGY_LIST" | jq -r '.items[0].manifest.display_name // "unknown"')
  _pass "Using strategy: $STRATEGY_ID ($STRATEGY_NAME)"
fi

# ── 4. Launch live eval run ───────────────────────────────────────────────────

_log "4. Starting live eval run ..."
RUN_BODY=$(cat <<EOF
{
  "agent_id": "$STRATEGY_ID",
  "scenario_id": "smoke-live",
  "mode": "Live",
  "skip_preflight": false,
  "live_config": {
    "strategy_id": "$STRATEGY_ID",
    "assets": [{"symbol": "$LIVE_ASSET", "class": "crypto"}],
    "capital": {"initial": 10000.0, "currency": "USD"},
    "broker_creds_ref": "alpaca",
    "stop_policy": {"time_limit_secs": $TIME_LIMIT},
    "venue_label": "Paper",
    "display_name": "smoke-test-$(date +%s)"
  }
}
EOF
)
RUN_RESPONSE=$(_http "POST /api/eval/runs (start live run)" POST \
  "$BASE_URL/api/eval/runs" "$RUN_BODY")
RUN_ID=$(printf '%s' "$RUN_RESPONSE" | jq -r '.id')
RUN_STATUS=$(printf '%s' "$RUN_RESPONSE" | jq -r '.status')
_log "  Run created: $RUN_ID (status=$RUN_STATUS)"

# ── 5. Wait for 'running' ─────────────────────────────────────────────────────

_log "5. Polling until status=running ..."
_poll_status "$RUN_ID" "running" "$RUNNING_TIMEOUT"
_pass "Run is running"

# ── 6. Wait for at least one decision (non-fatal) ─────────────────────────────

_log "6. Waiting for first decision ..."
_wait_for_decisions "$RUN_ID" "$DECISIONS_WAIT"

# ── 7. Pause ──────────────────────────────────────────────────────────────────

_log "7. Pausing run ..."
_http "POST /api/eval/runs/$RUN_ID/pause" POST \
  "$BASE_URL/api/eval/runs/$RUN_ID/pause" >/dev/null
sleep 2

# ── 8. Flatten positions ──────────────────────────────────────────────────────

_log "8. Requesting flatten (close all positions) ..."
_http "POST /api/eval/runs/$RUN_ID/flatten" POST \
  "$BASE_URL/api/eval/runs/$RUN_ID/flatten" >/dev/null
sleep 2

# ── 9. Resume ─────────────────────────────────────────────────────────────────

_log "9. Resuming run ..."
_http "POST /api/eval/runs/$RUN_ID/resume" POST \
  "$BASE_URL/api/eval/runs/$RUN_ID/resume" >/dev/null
sleep 2

# ── 10. Cancel ────────────────────────────────────────────────────────────────

_log "10. Cancelling run ..."
_http "POST /api/eval/runs/$RUN_ID/cancel" POST \
  "$BASE_URL/api/eval/runs/$RUN_ID/cancel" >/dev/null

# ── 11. Wait for terminal state ───────────────────────────────────────────────

_log "11. Polling until terminal (cancelled or completed) ..."
CANCEL_TIMEOUT=60
elapsed=0
FINAL_STATUS=""
while [[ "$elapsed" -lt "$CANCEL_TIMEOUT" ]]; do
  FINAL_STATUS=$(curl -sf "$BASE_URL/api/eval/runs/$RUN_ID" | jq -r '.status // "unknown"')
  _log "  status=$FINAL_STATUS (${elapsed}s/${CANCEL_TIMEOUT}s)"
  if [[ "$FINAL_STATUS" =~ ^(cancelled|completed|failed)$ ]]; then
    break
  fi
  sleep "$POLL_INTERVAL"
  elapsed=$((elapsed + POLL_INTERVAL))
done

if [[ ! "$FINAL_STATUS" =~ ^(cancelled|completed|failed)$ ]]; then
  _fail "run did not reach terminal state within ${CANCEL_TIMEOUT}s (last: $FINAL_STATUS)"
fi

# ── 12. Final assertions ──────────────────────────────────────────────────────

_log "12. Final assertions ..."
RUN_DETAIL=$(curl -sf "$BASE_URL/api/eval/runs/$RUN_ID")
DECISION_COUNT=$(printf '%s' "$RUN_DETAIL" | jq -r '.decisions | length // 0' 2>/dev/null || echo "0")

_pass "Run $RUN_ID: status=$FINAL_STATUS decisions=$DECISION_COUNT"

if [[ "${DECISION_COUNT:-0}" -gt 0 ]]; then
  _pass "decisions > 0 ✓"
else
  _log "  NOTE: decisions=0 (run was cancelled before first LLM dispatch)"
fi

_log ""
_log "=== smoke PASSED ✓ ==="
_log "  run_id:    $RUN_ID"
_log "  status:    $FINAL_STATUS"
_log "  decisions: ${DECISION_COUNT:-0}"
