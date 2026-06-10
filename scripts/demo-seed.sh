#!/usr/bin/env bash
# scripts/demo-seed.sh — Seeds the demo host with starter data for the Turing Hackathon.
#
# Idempotent: skips each section when the data is already present.
# Every demo screen needs real data; this script provides it on a fresh container.
#
# Requirements:
#   - xvision dashboard running at SEED_BASE_URL (dashboard port, default :8788)
#   - jq on PATH
#   - APCA_API_KEY_ID + APCA_API_SECRET_KEY set for the live paper run
#     (skip with SEED_SKIP_LIVE=1 if Alpaca creds are unavailable)
#
# Environment:
#   SEED_BASE_URL       dashboard URL (default: http://localhost:8788)
#   SEED_SKIP_LIVE      set to 1 to skip live paper run (offline demo prep)
#   SEED_SKIP_OPTIMIZER set to 1 to skip autooptimizer cycle (no LLM needed)
#   APCA_API_KEY_ID     Alpaca paper key id (required unless SEED_SKIP_LIVE=1)
#   APCA_API_SECRET_KEY Alpaca paper secret key (required unless SEED_SKIP_LIVE=1)
#
# What is seeded (in order):
#   1. Alpaca credentials (idempotent PUT)
#   2. 3 strategies — BollingerSqueeze (Haiku 4h), RSI Capitulation (Haiku 1h),
#      EMA Golden Cross (Haiku 1h) — each with distinct prompts
#   3. 1 completed backtest — BollingerSqueeze vs crypto-bull-q1-2025
#      with max_concurrent_positions=1 so supervisor notes show vetoes
#   4. 1 autooptimizer cycle launched against BollingerSqueeze (left running)
#   5. 1 live paper run — RSI Capitulation, BTC/USD (left running)
#
# Exit: 0 = all assertions passed, 1 = failure.
#
# Usage — local:
#   APCA_API_KEY_ID=PKxxx APCA_API_SECRET_KEY=xxx ./scripts/demo-seed.sh
#
# Usage — docker exec:
#   docker exec <container> bash /app/scripts/demo-seed.sh

set -euo pipefail

BASE_URL="${SEED_BASE_URL:-http://localhost:8788}"
SKIP_LIVE="${SEED_SKIP_LIVE:-0}"
SKIP_OPTIMIZER="${SEED_SKIP_OPTIMIZER:-0}"
POLL_INTERVAL=4
BACKTEST_TIMEOUT=300

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# ── helpers ───────────────────────────────────────────────────────────────────

_log()  { printf '[seed] %s\n'   "$*" >&2; }
_pass() { printf '[seed] ✓ %s\n' "$*" >&2; }
_warn() { printf '[seed] ⚠ %s\n' "$*" >&2; }
_fail() { printf '[seed] ✗ %s\n' "$*" >&2; exit 1; }

_require() { command -v "$1" >/dev/null 2>&1 || _fail "required tool not found: $1"; }

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

# GET without failing on non-2xx (returns body + stores status in LAST_STATUS).
_http_get_soft() {
  local url="$1"
  local tmpfile status
  tmpfile="$(mktemp)"
  status=$(curl -s -o "$tmpfile" -w '%{http_code}' -X GET "$url")
  LAST_STATUS="$status"
  cat "$tmpfile"
  rm -f "$tmpfile"
}

# ── preflight ─────────────────────────────────────────────────────────────────

_require curl
_require jq

_log "=== xvision demo seed ==="
_log "Dashboard: $BASE_URL"
[[ "$SKIP_LIVE" == "1" ]] && _log "(live paper run: SKIPPED)"
[[ "$SKIP_OPTIMIZER" == "1" ]] && _log "(autooptimizer: SKIPPED)"

# ── 1. Health check ───────────────────────────────────────────────────────────

_log ""
_log "1. Health check ..."
_http "GET /api/eval/runs (health)" GET "$BASE_URL/api/eval/runs?limit=1" >/dev/null

# ── 2. Seed Alpaca credentials (idempotent) ───────────────────────────────────

_log ""
_log "2. Alpaca credentials ..."
if [[ -n "${APCA_API_KEY_ID:-}" && -n "${APCA_API_SECRET_KEY:-}" ]]; then
  _http "PUT /api/settings/brokers/alpaca" POST \
    "$BASE_URL/api/settings/brokers/alpaca" \
    "{\"api_key_id\":\"$APCA_API_KEY_ID\",\"api_secret_key\":\"$APCA_API_SECRET_KEY\"}" \
    >/dev/null
else
  _log "  APCA creds not set — skipping (use APCA_API_KEY_ID + APCA_API_SECRET_KEY)"
fi

# ── 3. Create strategies ──────────────────────────────────────────────────────
#
# Each strategy: create agent → create strategy → attach agent → set
# asset_universe + cadence. Skips a strategy if display_name already exists.

_log ""
_log "3. Strategies ..."

# Returns the id of the first strategy matching display_name, or empty string.
_find_strategy() {
  local name="$1"
  curl -sf "$BASE_URL/api/strategies?limit=50" \
    | jq -r --arg n "$name" '.items[] | select(.manifest.display_name == $n) | .manifest.id' \
    | head -1
}

# Create one agent and return its id.
_create_agent() {
  local name="$1" provider="$2" model="$3" slot_name="$4" prompt="$5"
  local body
  body=$(jq -n \
    --arg name "$name" \
    --arg provider "$provider" \
    --arg model "$model" \
    --arg slot_name "$slot_name" \
    --arg prompt "$prompt" \
    '{
      name: $name,
      description: ("Demo: " + $name),
      tags: ["demo"],
      slots: [{
        name: $slot_name,
        provider: $provider,
        model: $model,
        system_prompt: $prompt,
        skill_ids: [],
        max_tokens: null,
        max_wall_ms: null
      }]
    }')
  _http "POST /api/agents ($name)" POST "$BASE_URL/api/agents" "$body" \
    | jq -r '.id'
}

# Create a blank strategy and return its id.
_create_strategy() {
  local display_name="$1"
  local body
  body=$(jq -n --arg name "$display_name" '{name: $name}')
  _http "POST /api/strategies ($display_name)" POST "$BASE_URL/api/strategies" "$body" \
    | jq -r '.id'
}

# Attach an agent to a strategy under a role.
_attach_agent() {
  local strategy_id="$1" agent_id="$2" role="$3"
  local body
  body=$(jq -n --arg agent_id "$agent_id" --arg role "$role" \
    '{agent_id: $agent_id, role: $role}')
  _http "POST /api/strategy/$strategy_id/agents" POST \
    "$BASE_URL/api/strategy/$strategy_id/agents" "$body" >/dev/null
}

# PATCH asset_universe + cadence.
_patch_manifest() {
  local strategy_id="$1" cadence="$2"
  shift 2
  local assets_json="$1"
  local body
  body=$(jq -n \
    --argjson assets "$assets_json" \
    --argjson cadence "$cadence" \
    '{asset_universe: $assets, decision_cadence_minutes: $cadence}')
  _http "PATCH /api/strategy/$strategy_id (manifest)" PATCH \
    "$BASE_URL/api/strategy/$strategy_id" "$body" >/dev/null
}

# PUT risk config.
_put_risk() {
  local strategy_id="$1" max_positions="$2" daily_loss_pct="$3"
  local body
  body=$(jq -n \
    --arg id "$strategy_id" \
    --argjson max_pos "$max_positions" \
    --argjson dlp "$daily_loss_pct" \
    '{
      id: $id,
      explicit: {
        max_concurrent_positions: $max_pos,
        daily_loss_kill_pct: $dlp
      }
    }')
  _http "PUT /api/strategy/$strategy_id/risk" PUT \
    "$BASE_URL/api/strategy/$strategy_id/risk" "$body" >/dev/null
}

# ── 3a. BollingerSqueeze ──────────────────────────────────────────────────────

BOLLINGER_NAME="Demo: Bollinger Squeeze"
BOLLINGER_ID="$(_find_strategy "$BOLLINGER_NAME")"

if [[ -n "$BOLLINGER_ID" ]]; then
  _pass "BollingerSqueeze already exists: $BOLLINGER_ID"
else
  _log "  Creating BollingerSqueeze ..."
  BOLLINGER_PROMPT="You are a momentum trader specializing in Bollinger Band squeeze breakouts.

Monitor BTC/USD and ETH/USD on the 4-hour chart. A 'squeeze' forms when the Bollinger Band width (upper minus lower, divided by middle) is below its 20-period average.

Decision rules:
- If a squeeze is resolving and price closes ABOVE the upper band → output action=long_open with size=0.4 and a confidence score.
- If a squeeze is resolving and price closes BELOW the lower band → output action=short_open with size=0.3 and a confidence score.
- If holding a long and price crosses back below the 20-period middle band → output action=long_close.
- If holding a short and price crosses back above the 20-period middle band → output action=short_close.
- Otherwise → output action=hold.

Always include a one-sentence rationale explaining the key indicator values that drove the decision."

  BOLLINGER_AGENT_ID="$(_create_agent "Bollinger Squeeze Trader" "anthropic" "claude-haiku-4-5-20251001" "trader" "$BOLLINGER_PROMPT")"
  BOLLINGER_ID="$(_create_strategy "$BOLLINGER_NAME")"
  _attach_agent "$BOLLINGER_ID" "$BOLLINGER_AGENT_ID" "trader"
  _patch_manifest "$BOLLINGER_ID" 240 '["BTC/USD","ETH/USD"]'
  _put_risk "$BOLLINGER_ID" 1 0.05
  _pass "BollingerSqueeze created: $BOLLINGER_ID"
fi

# ── 3b. RSI Capitulation ──────────────────────────────────────────────────────

RSI_NAME="Demo: RSI Capitulation"
RSI_ID="$(_find_strategy "$RSI_NAME")"

if [[ -n "$RSI_ID" ]]; then
  _pass "RSI Capitulation already exists: $RSI_ID"
else
  _log "  Creating RSI Capitulation ..."
  RSI_PROMPT="You are a contrarian trader using RSI oversold readings to identify capitulation bottoms.

Monitor BTC/USD and ETH/USD on the 1-hour chart.

Decision rules:
- RSI(14) < 25 AND current-bar volume > 1.5× its 10-bar average → output action=long_open, size=0.3, confidence=high.
- RSI(14) < 30 AND volume >= average → output action=long_open, size=0.2, confidence=medium.
- If holding a long AND RSI(14) > 58 → output action=long_close.
- If holding a long AND RSI(14) > 50 AND bars_held >= 6 → output action=long_close.
- This is a long-only strategy. Never output short_open.
- Otherwise → output action=hold.

Include confidence (high/medium/low) and a brief note on RSI value and volume ratio."

  RSI_AGENT_ID="$(_create_agent "RSI Capitulation Trader" "anthropic" "claude-haiku-4-5-20251001" "trader" "$RSI_PROMPT")"
  RSI_ID="$(_create_strategy "$RSI_NAME")"
  _attach_agent "$RSI_ID" "$RSI_AGENT_ID" "trader"
  _patch_manifest "$RSI_ID" 60 '["BTC/USD","ETH/USD"]'
  _put_risk "$RSI_ID" 2 0.08
  _pass "RSI Capitulation created: $RSI_ID"
fi

# ── 3c. EMA Golden Cross ──────────────────────────────────────────────────────

EMA_NAME="Demo: EMA Golden Cross"
EMA_ID="$(_find_strategy "$EMA_NAME")"

if [[ -n "$EMA_ID" ]]; then
  _pass "EMA Golden Cross already exists: $EMA_ID"
else
  _log "  Creating EMA Golden Cross ..."
  EMA_PROMPT="You are a trend-following trader using EMA(50)/EMA(200) crossovers.

Monitor BTC/USD on the 1-hour chart.

Decision rules:
- If EMA(50) crosses ABOVE EMA(200) this bar or the previous bar (golden cross) AND you are not already long → output action=long_open, size=0.5.
- If EMA(50) crosses BELOW EMA(200) this bar or the previous bar (death cross) AND you are not already short → output action=short_open, size=0.35.
- If holding a long AND a death cross occurs → output action=long_close first.
- If holding a short AND a golden cross occurs → output action=short_close first.
- In flat sideways conditions (EMA(50) within 0.5% of EMA(200)) → output action=hold to avoid whipsaw.
- Otherwise → output action=hold.

State clearly: current EMA(50) value, EMA(200) value, whether a crossover occurred, and your trend assessment."

  EMA_AGENT_ID="$(_create_agent "EMA Golden Cross Trader" "anthropic" "claude-haiku-4-5-20251001" "trader" "$EMA_PROMPT")"
  EMA_ID="$(_create_strategy "$EMA_NAME")"
  _attach_agent "$EMA_ID" "$EMA_AGENT_ID" "trader"
  _patch_manifest "$EMA_ID" 60 '["BTC/USD"]'
  _put_risk "$EMA_ID" 1 0.06
  _pass "EMA Golden Cross created: $EMA_ID"
fi

_log "  Strategies seeded: BollingerSqueeze=$BOLLINGER_ID RSI=$RSI_ID EMA=$EMA_ID"

# ── 4. Completed backtest with vetoes ─────────────────────────────────────────
#
# BollingerSqueeze vs crypto-bull-q1-2025 with max_concurrent_positions=1.
# Trades BTC/USD + ETH/USD; the second position is systematically vetoed.
# Waits for completion so the run appears in the eval history page.

_log ""
_log "4. Backtest (crypto-bull-q1-2025) ..."

# Check if a completed backtest already exists for BollingerSqueeze.
EXISTING_BT=$(curl -sf "$BASE_URL/api/eval/runs?limit=50" \
  | jq -r --arg sid "$BOLLINGER_ID" \
    '.items[] | select(.agent_id == $sid and .status == "completed") | .id' \
  | head -1)

if [[ -n "$EXISTING_BT" ]]; then
  _pass "Backtest already completed: $EXISTING_BT"
else
  _log "  Starting backtest ..."
  BT_BODY=$(jq -n \
    --arg agent_id "$BOLLINGER_ID" \
    '{
      agent_id: $agent_id,
      scenario_id: "crypto-bull-q1-2025",
      mode: "Backtest",
      skip_preflight: false
    }')
  BT_RESPONSE=$(_http "POST /api/eval/runs (backtest)" POST \
    "$BASE_URL/api/eval/runs" "$BT_BODY")
  BT_ID=$(printf '%s' "$BT_RESPONSE" | jq -r '.id')
  _log "  Backtest started: $BT_ID — waiting up to ${BACKTEST_TIMEOUT}s ..."

  elapsed=0
  BT_STATUS=""
  while [[ "$elapsed" -lt "$BACKTEST_TIMEOUT" ]]; do
    BT_STATUS=$(curl -sf "$BASE_URL/api/eval/runs/$BT_ID" \
      | jq -r '.status // "unknown"')
    _log "  status=$BT_STATUS (${elapsed}s/${BACKTEST_TIMEOUT}s)"
    if [[ "$BT_STATUS" =~ ^(completed|failed|cancelled)$ ]]; then
      break
    fi
    sleep "$POLL_INTERVAL"
    elapsed=$((elapsed + POLL_INTERVAL))
  done

  if [[ "$BT_STATUS" == "completed" ]]; then
    _pass "Backtest completed: $BT_ID"
  elif [[ "$BT_STATUS" =~ ^(failed|cancelled)$ ]]; then
    _warn "Backtest ended with status=$BT_STATUS (run_id=$BT_ID) — demo continues"
  else
    _warn "Backtest timed out after ${BACKTEST_TIMEOUT}s (last status=$BT_STATUS) — demo continues"
  fi
fi

# ── 5. Autooptimizer cycle ────────────────────────────────────────────────────
#
# Launch a cycle against BollingerSqueeze and leave it running.
# Short date window to keep bar-fetch cost low.

if [[ "$SKIP_OPTIMIZER" == "1" ]]; then
  _log ""
  _log "5. Autooptimizer — SKIPPED (SEED_SKIP_OPTIMIZER=1)"
else
  _log ""
  _log "5. Autooptimizer cycle ..."

  EXISTING_OPT=$(curl -sf "$BASE_URL/api/autooptimizer?limit=5" \
    | jq -r '.items[]? | select(.status == "running" or .status == "pending") | .id' \
    | head -1)

  if [[ -n "$EXISTING_OPT" ]]; then
    _pass "Autooptimizer cycle already running: $EXISTING_OPT"
  else
    _log "  Starting optimizer cycle for BollingerSqueeze ($BOLLINGER_ID) ..."
    OPT_BODY=$(jq -n \
      --arg sid "$BOLLINGER_ID" \
      '{
        strategy_id: $sid,
        experiments_per_cycle: 1,
        budget_usd: 0.75,
        day_start: "2025-01-01",
        day_end: "2025-01-14",
        baseline_start: "2025-01-01",
        baseline_end: "2025-01-14"
      }')
    OPT_RESPONSE=$(_http "POST /api/autooptimizer/run-cycle" POST \
      "$BASE_URL/api/autooptimizer/run-cycle" "$OPT_BODY")
    OPT_SESSION=$(printf '%s' "$OPT_RESPONSE" | jq -r '.session_id // "n/a"')
    _pass "Optimizer cycle launched (session=$OPT_SESSION) — running in background"
  fi
fi

# ── 6. Live paper run ─────────────────────────────────────────────────────────
#
# RSI Capitulation strategy, BTC/USD, Alpaca paper trading.
# Left running so the live eval page shows an active run.

if [[ "$SKIP_LIVE" == "1" ]]; then
  _log ""
  _log "6. Live paper run — SKIPPED (SEED_SKIP_LIVE=1)"
elif [[ -z "${APCA_API_KEY_ID:-}" || -z "${APCA_API_SECRET_KEY:-}" ]]; then
  _log ""
  _warn "6. Live paper run — SKIPPED (APCA_API_KEY_ID/APCA_API_SECRET_KEY not set)"
else
  _log ""
  _log "6. Live paper run (RSI Capitulation, BTC/USD) ..."

  EXISTING_LIVE=$(curl -sf "$BASE_URL/api/eval/runs?limit=20" \
    | jq -r --arg sid "$RSI_ID" \
      '.items[] | select(.agent_id == $sid and .status == "running") | .id' \
    | head -1)

  if [[ -n "$EXISTING_LIVE" ]]; then
    _pass "Live run already active: $EXISTING_LIVE"
  else
    _log "  Launching live paper run ..."
    LIVE_BODY=$(jq -n \
      --arg agent_id "$RSI_ID" \
      '{
        agent_id: $agent_id,
        scenario_id: "seed-live",
        mode: "Live",
        skip_preflight: false,
        live_config: {
          strategy_id: $agent_id,
          assets: [{"symbol": "BTC/USD", "class": "crypto"}],
          capital: {"initial": 10000.0, "currency": "USD"},
          broker_creds_ref: "alpaca",
          stop_policy: {"time_limit_secs": 3600},
          venue_label: "Paper",
          display_name: "demo-live-rsi-btc"
        }
      }')
    LIVE_RESPONSE=$(_http "POST /api/eval/runs (live)" POST \
      "$BASE_URL/api/eval/runs" "$LIVE_BODY")
    LIVE_ID=$(printf '%s' "$LIVE_RESPONSE" | jq -r '.id')

    # Wait up to 30s for status=running.
    elapsed=0
    LIVE_STATUS=""
    while [[ "$elapsed" -lt 30 ]]; do
      LIVE_STATUS=$(curl -sf "$BASE_URL/api/eval/runs/$LIVE_ID" \
        | jq -r '.status // "unknown"')
      if [[ "$LIVE_STATUS" == "running" || "$LIVE_STATUS" =~ ^(failed|cancelled)$ ]]; then
        break
      fi
      sleep 2
      elapsed=$((elapsed + 2))
    done

    if [[ "$LIVE_STATUS" == "running" ]]; then
      _pass "Live run active: $LIVE_ID (status=running)"
    else
      _warn "Live run $LIVE_ID status=$LIVE_STATUS (may still be starting — check dashboard)"
    fi
  fi
fi

# ── Summary ───────────────────────────────────────────────────────────────────

_log ""
_log "=== demo seed complete ==="
_log "  Bollinger Squeeze strategy : $BOLLINGER_ID"
_log "  RSI Capitulation strategy  : $RSI_ID"
_log "  EMA Golden Cross strategy  : $EMA_ID"
_log ""
_log "  Completed backtest (with vetoes) : available in eval history"
[[ "$SKIP_OPTIMIZER" != "1" ]] && _log "  Optimizer cycle : running (check /optimizer page)"
[[ "$SKIP_LIVE" != "1" && -n "${APCA_API_KEY_ID:-}" ]] && _log "  Live paper run  : running (check /eval page)"
_log ""
_log "Run demo-smoke.sh to verify the live paper stack end-to-end."
