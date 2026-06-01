#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/export-pattern-lineage.sh <autooptimizer_run_id> [--output path]

Reads `xvn optimizer inspect --json` and the produced Pattern row via
`xvn memory show --json`, then emits a small Markdown lineage report.
USAGE
}

if [[ $# -lt 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

run_id="$1"
shift
output=""
while [[ $# -gt 0 ]]; do
  case "$1" in
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

run_json="$(xvn optimizer inspect "$run_id" --json)"
pattern_id="$(
  JSON="$run_json" python3 - <<'PY'
import json
import os
print(json.loads(os.environ["JSON"])["pattern_id"])
PY
)"
pattern_json="$(xvn memory show "$pattern_id" --json)"

report="$(
  RUN_JSON="$run_json" PATTERN_JSON="$pattern_json" python3 - <<'PY'
import json
import os

run = json.loads(os.environ["RUN_JSON"])
pattern = json.loads(os.environ["PATTERN_JSON"])
obs = run.get("observation_ids") or []

print("# Pattern Lineage")
print()
print(f"- autooptimizer run: `{run['id']}`")
print(f"- namespace: `{run['namespace']}`")
print(f"- pattern id: `{run['pattern_id']}`")
print(f"- pattern promotion state: `{pattern.get('promotion_state') or 'active'}`")
print(f"- run promotion state: `{run['promotion_state']}`")
print(f"- status: `{run['status']}`")
print(f"- observation count: `{len(obs)}`")
if obs:
    print(f"- observation ids: `{', '.join(obs)}`")
print(f"- training window end: `{pattern.get('training_window_end')}`")
print(f"- gate verdict: `{run.get('gate_verdict')}`")
if run.get("gate_reason"):
    print(f"- gate reason: `{run['gate_reason']}`")
if run.get("gate_metric"):
    print(f"- gate metric: `{run['gate_metric']}`")
if run.get("delta_day") is not None:
    print(f"- delta day: `{run['delta_day']}`")
if run.get("delta_holdout") is not None:
    print(f"- delta holdout: `{run['delta_holdout']}`")
if run.get("finding_text"):
    print()
    print("## Finding")
    print()
    print(run["finding_text"])
if run.get("qualitative_finding_json"):
    print()
    print("## Finding JSON")
    print()
    print("```json")
    print(run["qualitative_finding_json"])
    print("```")
if run.get("judge_model") or run.get("judge_token_cost") is not None:
    print()
    print("## Judge")
    print()
    print(f"- model: `{run.get('judge_model')}`")
    print(f"- token cost: `{run.get('judge_token_cost')}`")
    print(f"- blinded metrics: `{run.get('finding_blinded_metrics')}`")
PY
)"

if [[ -n "$output" ]]; then
  printf '%s\n' "$report" > "$output"
else
  printf '%s\n' "$report"
fi
