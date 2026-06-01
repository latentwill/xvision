#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage:
  scripts/export-flywheel-velocity.sh --agent <id> [--days N] [--output path]
  scripts/export-flywheel-velocity.sh --namespace <ns> [--days N] [--output path]

Runs `xvn flywheel velocity --json` and emits a small Markdown report.
USAGE
}

args=()
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
      args+=("$1")
      shift
      ;;
  esac
done

json="$(xvn flywheel velocity "${args[@]}" --json)"
report="$(
  JSON="$json" python3 - <<'PY'
import json
import os

d = json.loads(os.environ["JSON"])
print("# Flywheel Velocity")
print()
print(f"- namespace: `{d['namespace']}`")
print(f"- days: `{d['days']}`")
print(f"- since: `{d['since']}`")
print(f"- observations captured: `{d['observations_captured']}`")
print(f"- patterns promoted: `{d['patterns_promoted']}`")
print(f"- patterns demoted: `{d['patterns_demoted']}`")
print(f"- autooptimizer runs: `{d['autooptimizer_runs']}`")
print(f"- optimized child agents: `{d['optimized_child_agents']}`")
print(f"- average lineage depth: `{d['average_lineage_depth']:.2f}`")
if d.get("latest_activity_at"):
    print(f"- latest activity: `{d['latest_activity_at']}`")
PY
)"

if [[ -n "$output" ]]; then
  printf '%s\n' "$report" > "$output"
else
  printf '%s\n' "$report"
fi
