#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/smoke-autooptimizer-distill.sh --agent <id> --pattern-text <text> --embedding-json '[1.0,0.0]' [options]
  scripts/smoke-autooptimizer-distill.sh --namespace <ns> --pattern-text <text> --embedding-json '[1.0,0.0]' [options]

Options:
  --scenario <id>          Filter source Observations by scenario_id.
  --run <id>               Filter source Observations by run_id.
  --limit <n>              Max Observation cohort size (default: xvn default).
  --min-observations <n>   Minimum cohort size (default: xvn default).
  --active                 Create an immediately recall-active Pattern.
  --output <path>          Write the smoke report to this path.

Runs `xvn autooptimizer run --json`, reads the run back with
`xvn autooptimizer inspect --json`, then records namespace-level flywheel
status with `xvn flywheel status --json`.
USAGE
}

run_args=()
scope_args=()
pattern_text=""
embedding_json=""
output=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --agent|--namespace)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      scope_args+=("$1" "$2")
      run_args+=("$1" "$2")
      shift 2
      ;;
    --pattern-text)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      pattern_text="$2"
      shift 2
      ;;
    --embedding-json)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      embedding_json="$2"
      shift 2
      ;;
    --scenario|--run|--limit|--min-observations)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      run_args+=("$1" "$2")
      shift 2
      ;;
    --active)
      run_args+=("$1")
      shift
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

if [[ ${#scope_args[@]} -ne 2 || -z "$pattern_text" || -z "$embedding_json" ]]; then
  usage
  exit 2
fi

run_json="$(xvn autooptimizer run "${run_args[@]}" \
  --pattern-text "$pattern_text" \
  --embedding-json "$embedding_json" \
  --json)"

run_id="$(
  JSON="$run_json" python3 - <<'PY'
import json
import os
print(json.loads(os.environ["JSON"])["id"])
PY
)"

inspect_json="$(xvn autooptimizer inspect "$run_id" --json)"
status_json="$(xvn flywheel status "${scope_args[@]}" --json)"

report="$(
  RUN_JSON="$run_json" INSPECT_JSON="$inspect_json" STATUS_JSON="$status_json" python3 - <<'PY'
import json
import os

run = json.loads(os.environ["RUN_JSON"])
inspect = json.loads(os.environ["INSPECT_JSON"])
status = json.loads(os.environ["STATUS_JSON"])
obs = inspect.get("observation_ids") or []

print("# AutoOptimizer Distill Smoke")
print()
print(f"- run: `{inspect['id']}`")
print(f"- namespace: `{inspect['namespace']}`")
print(f"- status: `{inspect['status']}`")
print(f"- pattern: `{inspect['pattern_id']}`")
print(f"- promotion state: `{inspect['promotion_state']}`")
print(f"- observations: `{len(obs)}`")
print(f"- pattern text: `{inspect['pattern_text']}`")
print()
print("## Flywheel Status")
print()
print(f"- observations: `{status['observations']}`")
print(f"- active patterns: `{status['active_patterns']}`")
print(f"- staged patterns: `{status['staged_patterns']}`")
print(f"- autooptimizer runs: `{status['autooptimizer_runs']}`")
if status.get("latest_autooptimizer_run_id"):
    print(f"- latest autooptimizer run: `{status['latest_autooptimizer_run_id']}`")
if run.get("error"):
    print()
    print("## Error")
    print()
    print(run["error"])
PY
)"

if [[ -n "$output" ]]; then
  printf '%s\n' "$report" > "$output"
else
  printf '%s\n' "$report"
fi
