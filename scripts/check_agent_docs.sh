#!/usr/bin/env bash
set -euo pipefail

README=README.md

grep -q "## For Agents" "$README"
grep -q "MANUAL.md" "$README"
grep -q "FOLLOWUPS.md" "$README"
grep -q ".claude/skills/xvision/SKILL.md" "$README"
grep -q "xvn --help" "$README"
grep -q "xvn.tail2bb69.ts.net" "$README"
