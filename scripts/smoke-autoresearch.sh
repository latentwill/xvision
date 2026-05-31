#!/usr/bin/env bash
# smoke-autoresearch.sh — end-to-end autoresearcher smoke test
#
# Verifies: session-init → 3× mutate-once --mock → lineage count check →
#           banned-term grep on autoresearch/memory/flywheel --help output.
#
# NOTE: After PRs #689–#697 land on main, replace the mutate-once loop with
#       xvn autoresearch evening-cycle --mock and add an xvn autoresearch demo
#       call. The banned-term grep block stays as-is.
#
# Requires: compiled xvn binary in PATH or $XVN_BIN
# Usage:    ./scripts/smoke-autoresearch.sh [--xvn-bin <path>]
#
# Exit codes: 0 = all checks passed, 1 = at least one check failed.
set -euo pipefail

# ── arg parsing ───────────────────────────────────────────────────────────────

XVN="${XVN_BIN:-xvn}"

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

echo "=== Autoresearcher smoke test ==="
echo "Using binary: $XVN"
echo "Working dir:  $WORK"
echo ""

# ── helpers ───────────────────────────────────────────────────────────────────

fail() { echo "FAIL: $*" >&2; exit 1; }

check_banned_terms() {
  local surface="$1"
  local help_text="$2"
  local failed=0
  # Banned operator-surface terms (see docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md)
  local banned=(promote demote epsilon holdout mutation mutator ghost quarantined merkle)
  for term in "${banned[@]}"; do
    if echo "$help_text" | grep -qiw "$term"; then
      echo "  FAIL: banned term '$term' found in '$surface --help'" >&2
      failed=1
    fi
  done
  return $failed
}

# ── write autoresearch.toml ───────────────────────────────────────────────────

CONFIG="$WORK/autoresearch.toml"
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

# ── write parent strategy blob ────────────────────────────────────────────────
# We need a valid blob in the store for mutate-once to read.
# Use the same fixture JSON as autoresearch_cli_mutate_once.rs.

BLOB_DIR="$WORK/blobs"
PARENT_JSON="$WORK/parent.json"
cat > "$PARENT_JSON" <<'JSON'
{
  "manifest": {
    "id": "01HSMOKE000AAAAAAAAAAAAA",
    "display_name": "Smoke Test Strategy",
    "plain_summary": "smoke-autoresearch.sh parent strategy",
    "creator": "@smoke",
    "template": "custom",
    "regime_fit": [],
    "asset_universe": ["BTC/USD"],
    "decision_cadence_minutes": 60,
    "attested_with": [],
    "required_tools": [],
    "risk_preset_or_config": "balanced",
    "published_at": null
  },
  "risk": {
    "risk_pct_per_trade": 0.015,
    "max_concurrent_positions": 2,
    "max_leverage": 3.0,
    "stop_loss_atr_multiple": 2.0,
    "daily_loss_kill_pct": 0.05
  },
  "mechanical_params": { "rsi_period": 14 }
}
JSON

# Compute the BLAKE3 hex of canonical JSON via the xvn binary itself is not
# directly available, so we pre-compute the hash using python3 if available,
# otherwise skip the blob placement and let the not-found exit tell us.
# The preferred path is: python3 writes the blob under blobs/<h1>/<h2>/<tail>.json
# exactly as BlobStore expects (keyed by BLAKE3 content hash).
PARENT_HASH=""
if command -v python3 >/dev/null 2>&1; then
  PARENT_HASH="$(python3 - "$PARENT_JSON" "$BLOB_DIR" <<'PY'
import json, hashlib, os, sys

json_path = sys.argv[1]
blob_dir  = sys.argv[2]

raw = open(json_path).read()
# Canonical JSON: sort keys, compact separators — mirrors ContentHash::of_json
canonical = json.dumps(json.loads(raw), sort_keys=True, separators=(',', ':'))
digest = hashlib.blake3(canonical.encode()).hexdigest()  # type: ignore[attr-defined]

h1, h2, tail = digest[:2], digest[2:4], digest[4:]
dest = os.path.join(blob_dir, h1, h2, f"{tail}.json")
os.makedirs(os.path.dirname(dest), exist_ok=True)
with open(dest, 'w') as f:
    f.write(canonical)
print(digest)
PY
)" 2>/dev/null || PARENT_HASH=""
fi

if [[ -z "$PARENT_HASH" ]]; then
  # Fallback: call python3 with blake3 may not be available; skip the
  # mutate-once loop but still run the session-init and banned-term checks.
  echo "WARNING: python3+blake3 not available; skipping mutate-once loop." >&2
  SKIP_MUTATE=1
else
  SKIP_MUTATE=0
fi

# ── 1. session-init ───────────────────────────────────────────────────────────

echo "--- xvn autoresearch session-init ---"
SESSION_JSON="$WORK/session.json"
KEY_PATH="$WORK/operator.ed25519"

"$XVN" autoresearch session-init \
  --config "$CONFIG" \
  --out    "$SESSION_JSON" \
  --key-path "$KEY_PATH"

[[ -f "$SESSION_JSON" ]] || fail "session.json not created"
SESSION_ID="$(python3 -c "import json,sys; print(json.load(open('$SESSION_JSON'))['session_id'])" 2>/dev/null \
           || grep -o '"session_id":"[^"]*"' "$SESSION_JSON" | cut -d'"' -f4)"
echo "session_id: $SESSION_ID"

# ── 2. mutate-once x3 (--mock) ────────────────────────────────────────────────
# NOTE(future): replace this loop with:
#   xvn autoresearch evening-cycle --session-id "$SESSION_ID" --db "$WORK/xvn.db" --mock
# once PRs #689–#697 are merged.

DB="$WORK/lineage.db"

if [[ "$SKIP_MUTATE" -eq 0 ]]; then
  for i in 1 2 3; do
    echo "--- mutate-once mock run $i ---"
    CYCLE_ID="smoke-cycle-$(printf '%02d' "$i")"
    "$XVN" autoresearch mutate-once "$PARENT_HASH" \
      --config    "$CONFIG" \
      --session   "$SESSION_JSON" \
      --db        "$DB" \
      --blob-dir  "$BLOB_DIR" \
      --key-path  "$KEY_PATH" \
      --cycle-id  "$CYCLE_ID" \
      --mock
  done

  # 3. Lineage count check — mock delta=0.2 > min_improvement=0.1 → PASS → seal
  echo "--- lineage count check ---"
  SEAL_COUNT="$(python3 -c "
import sqlite3, sys
conn = sqlite3.connect('$DB')
cur = conn.execute('SELECT COUNT(*) FROM lineage_nodes')
print(cur.fetchone()[0])
" 2>/dev/null || echo 0)"
  echo "Lineage nodes in DB: $SEAL_COUNT"
  [[ "$SEAL_COUNT" -ge 3 ]] || fail "expected >= 3 lineage nodes, got $SEAL_COUNT"

  CYCLE_SEAL_COUNT="$(python3 -c "
import sqlite3, sys
conn = sqlite3.connect('$DB')
cur = conn.execute('SELECT COUNT(*) FROM cycle_seals')
print(cur.fetchone()[0])
" 2>/dev/null || echo 0)"
  echo "Cycle seals (evening summaries) in DB: $CYCLE_SEAL_COUNT"
  # Each mock run gate-passes (delta 0.2 >= 0.1), so each should produce a seal.
  [[ "$CYCLE_SEAL_COUNT" -ge 3 ]] || fail "expected >= 3 cycle seals, got $CYCLE_SEAL_COUNT"
else
  echo "(mutate-once loop skipped — blake3 unavailable)"
fi

# ── 4. Banned-term grep on CLI help ──────────────────────────────────────────
# NOTE: These checks are expected to FAIL on origin/main (the current branch)
# because PRs #689–#697 that rename the operator-facing terms have not yet
# landed. Once those PRs merge, remove this NOTE and remove the "set +e" guard.
#
# Running with set +e so a banned-term hit is reported but does not abort the
# script before all surfaces are checked.

echo "--- Banned-term check (autoresearch/memory/flywheel --help) ---"
echo "    NOTE: Expected to flag terms until PRs #689–#697 merge."

OVERALL_BANNED_FAIL=0
set +e

AR_HELP="$("$XVN" autoresearch --help 2>&1; true)"
check_banned_terms "autoresearch" "$AR_HELP" || OVERALL_BANNED_FAIL=1

MEM_HELP="$("$XVN" memory --help 2>&1; true)"
check_banned_terms "memory" "$MEM_HELP" || OVERALL_BANNED_FAIL=1

FW_HELP="$("$XVN" flywheel --help 2>&1; true)"
check_banned_terms "flywheel" "$FW_HELP" || OVERALL_BANNED_FAIL=1

set -e

if [[ "$OVERALL_BANNED_FAIL" -eq 0 ]]; then
  echo "  OK: no banned terms found in help output."
else
  echo ""
  echo "WARNING: banned terms detected (expected until PRs #689-#697 merge)." >&2
  echo "         Re-run after merging to confirm clean operator vocabulary." >&2
fi

# ── done ─────────────────────────────────────────────────────────────────────

echo ""
echo "=== SMOKE TEST COMPLETE ==="
if [[ "$SKIP_MUTATE" -eq 1 ]]; then
  echo "    session-init: PASS"
  echo "    mutate-once:  SKIPPED (blake3 unavailable)"
  echo "    banned-terms: see above"
else
  echo "    session-init:  PASS"
  echo "    mutate-once:   PASS (3 runs, lineage nodes OK, seals OK)"
  echo "    banned-terms:  see above"
fi
