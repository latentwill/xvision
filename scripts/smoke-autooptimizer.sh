#!/usr/bin/env bash
# smoke-autooptimizer.sh — end-to-end autooptimizer smoke test
#
# Verifies: session-init -> evening-cycle --mock -> demo replay ->
#           banned-term grep on autooptimizer/memory/flywheel --help output.
#
# Requires: compiled xvn binary in PATH or $XVN_BIN
# Usage:    ./scripts/smoke-autooptimizer.sh [--xvn-bin <path>]
#
# Exit codes: 0 = all checks passed, 1 = at least one check failed.
set -euo pipefail

# ── arg parsing ───────────────────────────────────────────────────────────────

XVN="${XVN_BIN:-xvn}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --xvn-bin)
      [[ $# -ge 2 ]] || { echo "error: --xvn-bin requires a value" >&2; exit 2; }
      XVN="$2"
      shift 2
      ;;
    --help|-h)
      sed -n '2,/^$/p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# ── temp workspace ────────────────────────────────────────────────────────────

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "=== AutoOptimizer smoke test ==="
echo "Using binary: $XVN"
echo "Working dir:  $WORK"
echo ""

# ── helpers ───────────────────────────────────────────────────────────────────

fail() { echo "FAIL: $*" >&2; exit 1; }

check_banned_terms() {
  local surface="$1"
  local help_text="$2"
  local failed=0
  # Banned operator-surface terms (see docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md)
  local banned=(promote demote epsilon holdout mutation mutator ghost quarantined merkle)
  for term in "${banned[@]}"; do
    if echo "$help_text" | grep -qiw "$term"; then
      echo "  FAIL: banned term '$term' found in '$surface --help'" >&2
      failed=1
    fi
  done
  return $failed
}

# ── write autooptimizer.toml ───────────────────────────────────────────────────

CONFIG="$WORK/autooptimizer.toml"
cat > "$CONFIG" <<'TOML'
min_improvement = 0.1

[baseline_untouched_window]
start = "2025-09-01"
end   = "2025-12-01"

[day_window]
start = "2024-01-01"
end   = "2025-09-01"

[mutator]
provider   = "test"
model      = "test-model"
max_retries = 2
TOML

# ── 1. session-init ───────────────────────────────────────────────────────────

echo "--- xvn optimizer session-init ---"
SESSION_JSON="$WORK/session.json"
KEY_PATH="$WORK/operator.ed25519"

"$XVN" optimizer session-init \
  --config "$CONFIG" \
  --out    "$SESSION_JSON" \
  --key-path "$KEY_PATH"

[[ -f "$SESSION_JSON" ]] || fail "session.json not created"
SESSION_ID="$(python3 -c "import json,sys; print(json.load(open('$SESSION_JSON'))['session_id'])" 2>/dev/null \
           || grep -o '"session_id":"[^"]*"' "$SESSION_JSON" | cut -d'"' -f4)"
echo "session_id: $SESSION_ID"

# ── 2. evening-cycle (--mock) ─────────────────────────────────────────────────

DB="$WORK/lineage.db"

echo "--- xvn optimizer evening-cycle --mock ---"
"$XVN" optimizer evening-cycle \
  --session-id "$SESSION_ID" \
  --config "$CONFIG" \
  --db "$DB" \
  --mock

# 3. Seal check.
echo "--- cycle persistence check ---"
NODE_COUNT="$(python3 -c "
import sqlite3, sys
conn = sqlite3.connect('$DB')
cur = conn.execute('SELECT COUNT(*) FROM lineage_nodes')
print(cur.fetchone()[0])
" 2>/dev/null || echo 0)"
echo "Lineage nodes in DB: $NODE_COUNT"

CYCLE_SEAL_COUNT="$(python3 -c "
import sqlite3, sys
conn = sqlite3.connect('$DB')
cur = conn.execute('SELECT COUNT(*) FROM cycle_seals')
print(cur.fetchone()[0])
" 2>/dev/null || echo 0)"
echo "Cycle seals (evening summaries) in DB: $CYCLE_SEAL_COUNT"
[[ "$CYCLE_SEAL_COUNT" -ge 1 ]] || fail "expected >= 1 cycle seal, got $CYCLE_SEAL_COUNT"

# ── 4. demo replay ───────────────────────────────────────────────────────────

echo "--- xvn optimizer demo ---"
"$XVN" optimizer demo \
  --fixture "$REPO_ROOT/data/probes/autooptimizer/replay-fixture.json" \
  >/dev/null

# ── 5. Banned-term grep on CLI help ──────────────────────────────────────────

echo "--- Banned-term check (autooptimizer/memory/flywheel --help) ---"

OVERALL_BANNED_FAIL=0
set +e

AR_HELP="$("$XVN" optimizer --help 2>&1; true)"
check_banned_terms "autooptimizer" "$AR_HELP" || OVERALL_BANNED_FAIL=1

MEM_HELP="$("$XVN" memory --help 2>&1; true)"
check_banned_terms "memory" "$MEM_HELP" || OVERALL_BANNED_FAIL=1

FW_HELP="$("$XVN" flywheel --help 2>&1; true)"
check_banned_terms "flywheel" "$FW_HELP" || OVERALL_BANNED_FAIL=1

set -e

if [[ "$OVERALL_BANNED_FAIL" -eq 0 ]]; then
  echo "  OK: no banned terms found in help output."
else
  fail "banned terms detected in operator-facing help output"
fi

# ── done ─────────────────────────────────────────────────────────────────────

echo ""
echo "=== SMOKE TEST COMPLETE ==="
echo "    session-init:   PASS"
echo "    evening-cycle:  PASS"
echo "    demo:           PASS"
echo "    banned-terms:   PASS"
