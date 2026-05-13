#!/usr/bin/env bash
set -euo pipefail

README=README.md

reject() {
  local pattern=$1
  local file=$2
  if grep -q "$pattern" "$file"; then
    echo "forbidden pattern '$pattern' found in $file" >&2
    exit 1
  fi
}

grep -q "## For Agents" "$README"
grep -q "MANUAL.md" "$README"
grep -q "FOLLOWUPS.md" "$README"
grep -q ".claude/skills/xvision/SKILL.md" "$README"
grep -q "xvn --help" "$README"
grep -q "xvn.tail2bb69.ts.net" "$README"

grep -q "/eval-runs/compare" "$README"
reject "/eval/compare" "$README"

reject "xvn key issue" "$README"
reject "xvn budget set" "$README"
reject "xvn run --agent" "$README"
reject "xvn audit agent" "$README"
reject "xvn setup" "$README"
reject "buy_and_hold" "$README"

grep -q "/eval-runs/compare" frontend/README.md
reject "/eval/compare" frontend/README.md

reject "StrategyBundle" .claude/skills/xvision/SKILL.md
reject "xvn setup" .claude/skills/xvision/SKILL.md
reject "StrategyBundle" .claude/skills/xvision/references/architecture.md
reject "trader-arm" .claude/skills/xvision/references/cli.md
reject "strategy validate --id" .claude/skills/xvision/references/cli.md
reject "provider set-default" .claude/skills/xvision/references/cli.md
reject "provider rm" .claude/skills/xvision/references/cli.md
reject "provider add .* --model" .claude/skills/xvision/references/cli.md
reject "StrategyBundle" frontend/README.md
reject "StrategyBundle" MANUAL.md
reject "xvn kill" MANUAL.md
reject "xvn emergency-close" MANUAL.md
reject "xvn audit agent" MANUAL.md
reject "xvn setup" MANUAL.md
