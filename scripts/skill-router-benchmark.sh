#!/usr/bin/env bash
# Validate skill-fixtures.json structural correctness.
# Does NOT run prompts through a live LLM.
# Exit 0 = all fixtures pass; exit 1 = any failure.
set -euo pipefail

FIXTURES_PATH="${1:-.claude/skills/xvision/skill-fixtures.json}"
SKILLS_DIR=".claude/skills/xvision"
MAX_FIXTURES=100

assert_file_exists() {
  [[ -f "$1" ]] || { echo "FAIL: file not found: $1" >&2; exit 1; }
}

assert_file_exists "$FIXTURES_PATH"
command -v jq >/dev/null 2>&1 || { echo "FAIL: jq is required but not found" >&2; exit 1; }

# Build registry as newline-separated list of known skill names.
REGISTRY_FILE=$(mktemp /tmp/skill-registry.XXXXXX)
for skill_file in "$SKILLS_DIR"/*/SKILL.md; do
  [[ -f "$skill_file" ]] || continue
  name=$(grep -m1 '^name:' "$skill_file" | sed 's/^name:[[:space:]]*//' | tr -d '[:space:]')
  [[ -n "$name" ]] && echo "$name" >> "$REGISTRY_FILE"
done

REGISTRY_COUNT=$(wc -l < "$REGISTRY_FILE" | tr -d '[:space:]')
if [[ "$REGISTRY_COUNT" -eq 0 ]]; then
  echo "FAIL: no skills found under $SKILLS_DIR" >&2
  rm -f "$REGISTRY_FILE"
  exit 1
fi

echo "Registry (${REGISTRY_COUNT} skills): $(tr '\n' ' ' < "$REGISTRY_FILE")"

skill_known() {
  grep -qx "$1" "$REGISTRY_FILE"
}

# Count fixtures (hard cap).
TOTAL=$(jq 'length' "$FIXTURES_PATH")
if [[ "$TOTAL" -gt "$MAX_FIXTURES" ]]; then
  echo "FAIL: fixture count $TOTAL exceeds MAX_FIXTURES=$MAX_FIXTURES" >&2
  rm -f "$REGISTRY_FILE"
  exit 1
fi

PASS=0
FAIL=0

for i in $(seq 0 $((TOTAL - 1))); do
  fixture=$(jq ".[$i]" "$FIXTURES_PATH")

  id=$(echo "$fixture" | jq -r '.id // empty')
  prompt=$(echo "$fixture" | jq -r '.prompt // empty')
  expected=$(echo "$fixture" | jq -r '.expected_skill // empty')
  prefix="fixture[$i]"
  ok=true

  if [[ -z "$id" ]]; then
    echo "FAIL $prefix: missing required field 'id'"
    ok=false
  fi

  if [[ -z "$prompt" ]]; then
    echo "FAIL $prefix id=${id:-?}: missing required field 'prompt'"
    ok=false
  fi

  if [[ -z "$expected" ]]; then
    echo "FAIL $prefix id=${id:-?}: missing required field 'expected_skill'"
    ok=false
  elif ! skill_known "$expected"; then
    echo "FAIL $prefix id=${id:-?}: expected_skill '$expected' not in registry"
    ok=false
  fi

  snot_count=$(echo "$fixture" | jq '.should_not_trigger | length // 0')
  for j in $(seq 0 $((snot_count - 1))); do
    snot=$(echo "$fixture" | jq -r ".should_not_trigger[$j]")
    if ! skill_known "$snot"; then
      echo "FAIL $prefix id=${id:-?}: should_not_trigger[$j] '$snot' not in registry"
      ok=false
    fi
  done

  if $ok; then
    echo "PASS id=$id expected=$expected"
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
  fi
done

rm -f "$REGISTRY_FILE"
echo ""
echo "Results: $PASS passed, $FAIL failed (of $TOTAL fixtures)"
[[ $FAIL -eq 0 ]]
