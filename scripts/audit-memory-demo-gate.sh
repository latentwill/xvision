#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/audit-memory-demo-gate.sh <optimization_id> (--agent <id> | --namespace <ns>) \
    --parent-dev-score N --child-dev-score N \
    --parent-holdout-score N --child-holdout-score N \
    [--dev-metric name] [--holdout-metric name] [--gate-epsilon N] [--reason text] \
    [--output path]

Records an optimizer dev/holdout gate with `xvn optimize memory-demos-gate`,
then reads `xvn flywheel lineage --json` and verifies the same gate was
persisted on the selected optimization row.
USAGE
}

if [[ $# -lt 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

optimization_id="$1"
shift

scope_args=()
gate_args=()
output=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --agent|--namespace)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      scope_args+=("$1" "$2")
      shift 2
      ;;
    --parent-dev-score|--child-dev-score|--parent-holdout-score|--child-holdout-score|--dev-metric|--holdout-metric|--gate-epsilon|--reason)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      gate_args+=("$1" "$2")
      shift 2
      ;;
    --output)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      output="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$optimization_id" || ${#scope_args[@]} -eq 0 ]]; then
  usage
  exit 2
fi

gate_json="$(xvn optimize memory-demos-gate "$optimization_id" "${gate_args[@]}" --json)"
lineage_json="$(xvn flywheel lineage "${scope_args[@]}" --limit 100 --json)"

report="$(
  GATE_JSON="$gate_json" LINEAGE_JSON="$lineage_json" python3 - <<'PY'
import json
import os

gate = json.loads(os.environ["GATE_JSON"])
lineage = json.loads(os.environ["LINEAGE_JSON"])
optimization_id = gate["optimization_id"]
matches = [
    item for item in lineage.get("items", [])
    if item.get("optimization_id") == optimization_id
]
if not matches:
    raise SystemExit(f"optimization {optimization_id} not found in lineage")
item = matches[0]

checks = {
    "gate_verdict": gate["gate_verdict"],
    "delta_dev": gate["delta_dev"],
    "delta_holdout": gate["delta_holdout"],
    "gate_epsilon": gate["gate_epsilon"],
}
for key, expected in checks.items():
    actual = item.get(key)
    if isinstance(expected, float):
        if actual is None or abs(float(actual) - expected) > 1e-9:
            raise SystemExit(f"lineage {key} mismatch: expected {expected}, got {actual}")
    elif actual != expected:
        raise SystemExit(f"lineage {key} mismatch: expected {expected}, got {actual}")

print(json.dumps({
    "status": "ok",
    "namespace": lineage.get("namespace"),
    "optimization_id": optimization_id,
    "gate_verdict": gate["gate_verdict"],
    "dev_metric": gate["dev_metric"],
    "holdout_metric": gate["holdout_metric"],
    "parent_dev_score": gate["parent_dev_score"],
    "child_dev_score": gate["child_dev_score"],
    "parent_holdout_score": gate["parent_holdout_score"],
    "child_holdout_score": gate["child_holdout_score"],
    "gate_epsilon": gate["gate_epsilon"],
    "delta_dev": gate["delta_dev"],
    "delta_holdout": gate["delta_holdout"],
    "gate_reason": gate["gate_reason"],
    "gated_at": gate["gated_at"],
    "lineage_status": item.get("status"),
    "lineage_train_hash": item.get("train_hash"),
    "lineage_dev_hash": item.get("dev_hash"),
    "lineage_holdout_hash": item.get("holdout_hash"),
}, indent=2, sort_keys=True))
PY
)"

if [[ -n "$output" ]]; then
  printf '%s\n' "$report" > "$output"
else
  printf '%s\n' "$report"
fi
