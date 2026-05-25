#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/audit-memory-demos.sh [--output path] --agent <id> [xvn optimize memory-demos flags...]

Runs `xvn optimize memory-demos --json`, then verifies the returned
train/dev/holdout Observation id sets are disjoint and carry sha256 hashes.
The emitted JSON includes the exact split ids so the demo/holdout proof can
be persisted in the evidence ledger. Gate verdict proof is captured through
`xvn optimize memory-demos-gate` and `xvn flywheel lineage`.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" || "$#" -eq 0 ]]; then
  usage
  exit 0
fi

output=""
xvn_args=()
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
      xvn_args+=("$1")
      shift
      ;;
  esac
done

if [[ ${#xvn_args[@]} -eq 0 ]]; then
  usage
  exit 2
fi

json="$(xvn optimize memory-demos "${xvn_args[@]}" --json)"

report="$(
  JSON="$json" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["JSON"])
train = set(payload.get("train_observation_ids") or payload.get("observation_ids") or [])
dev = set(payload.get("dev_observation_ids") or [])
holdout = set(payload.get("holdout_observation_ids") or [])

overlaps = {
    "train_dev": sorted(train & dev),
    "train_holdout": sorted(train & holdout),
    "dev_holdout": sorted(dev & holdout),
}
bad = {k: v for k, v in overlaps.items() if v}
if bad:
    raise SystemExit(f"memory demo split overlap: {bad}")

for key in ("train_hash", "dev_hash", "holdout_hash"):
    value = payload.get(key)
    if not isinstance(value, str) or not value.startswith("sha256:"):
        raise SystemExit(f"missing {key}: expected sha256:<hex>")

print(json.dumps({
    "status": "ok",
    "optimization_id": payload.get("optimization_id"),
    "target_agent_id": payload.get("target_agent_id"),
    "child_agent_id": payload.get("child_agent_id"),
    "demo_source": payload.get("demo_source"),
    "holdout_split": payload.get("holdout_split"),
    "cohort_query": payload.get("cohort_query"),
    "train_count": len(train),
    "dev_count": len(dev),
    "holdout_count": len(holdout),
    "train_observation_ids": sorted(train),
    "dev_observation_ids": sorted(dev),
    "holdout_observation_ids": sorted(holdout),
    "train_hash": payload.get("train_hash"),
    "dev_hash": payload.get("dev_hash"),
    "holdout_hash": payload.get("holdout_hash"),
}, indent=2, sort_keys=True))
PY
)"

if [[ -n "$output" ]]; then
  printf '%s\n' "$report" > "$output"
else
  printf '%s\n' "$report"
fi
