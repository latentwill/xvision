#!/usr/bin/env bash
# Dead Strategy Exhumer — cross-reference strategy files vs eval/agent run history.
# Outputs an orphan inventory with confidence levels. No deletions performed.
#
# Requirements:
#   - $XVN_HOME must be set (or pass --home <path>)
#   - sqlite3 must be on PATH
#   - Engine DB at $XVN_HOME/engine.db
#   - Strategy JSON files at $XVN_HOME/strategies/*.json (or subdirs)

set -euo pipefail

XVN_HOME="${XVN_HOME:-}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --home) XVN_HOME="$2"; shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

if [ -z "$XVN_HOME" ]; then
    echo "Error: XVN_HOME is not set. Pass --home <path> or export XVN_HOME." >&2
    exit 1
fi

ENGINE_DB="$XVN_HOME/engine.db"
STRATEGY_DIR="$XVN_HOME/strategies"

if [ ! -f "$ENGINE_DB" ]; then
    echo "Error: engine DB not found at $ENGINE_DB" >&2
    exit 1
fi

if [ ! -d "$STRATEGY_DIR" ]; then
    echo "NULL RESULT: strategy directory not found at $STRATEGY_DIR"
    echo "No strategy files to cross-reference."
    exit 0
fi

strategy_files=()
while IFS= read -r -d '' f; do
    strategy_files+=("$f")
done < <(find "$STRATEGY_DIR" -name "*.json" -print0 2>/dev/null)

total=${#strategy_files[@]}
if [ "$total" -eq 0 ]; then
    echo "NULL RESULT: No strategy JSON files found in $STRATEGY_DIR."
    exit 0
fi

echo "=== Dead Strategy Exhumer ==="
echo "Strategy dir:  $STRATEGY_DIR ($total files)"
echo "Engine DB:     $ENGINE_DB"
echo ""

CONFIRMED_ORPHAN=0
LIKELY_DORMANT=0
ACTIVE=0

for filepath in "${strategy_files[@]}"; do
    filename=$(basename "$filepath" .json)
    agent_id=$(python3 -c "import json,sys; d=json.load(open('$filepath')); print(d.get('id', d.get('agent_id', '')))" 2>/dev/null || echo "")

    if [ -z "$agent_id" ]; then
        echo "UNKNOWN (no id field): $filename"
        continue
    fi

    in_eval_runs=$(sqlite3 "$ENGINE_DB" \
        "SELECT COUNT(*) FROM eval_runs WHERE agent_id='$agent_id';" 2>/dev/null || echo "0")
    in_agent_runs=$(sqlite3 "$ENGINE_DB" \
        "SELECT COUNT(*) FROM agent_runs WHERE strategy_id='$agent_id';" 2>/dev/null || echo "0")

    if [ "$in_eval_runs" -gt 0 ] || [ "$in_agent_runs" -gt 0 ]; then
        echo "ACTIVE           $agent_id  ($filename)  eval_runs=$in_eval_runs agent_runs=$in_agent_runs"
        ACTIVE=$((ACTIVE + 1))
    elif [ "$in_agent_runs" -eq 0 ] && [ "$in_eval_runs" -eq 0 ]; then
        echo "CONFIRMED_ORPHAN $agent_id  ($filename)  eval_runs=0 agent_runs=0"
        CONFIRMED_ORPHAN=$((CONFIRMED_ORPHAN + 1))
    fi
done

echo ""
echo "Summary: $ACTIVE active, $CONFIRMED_ORPHAN confirmed orphan(s), $LIKELY_DORMANT likely dormant"
echo "Safe cleanup: archive JSON (do not delete) and mark archived=true in the agents table."
