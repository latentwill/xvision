#!/usr/bin/env bash
set -euo pipefail

README=README.md
SKILL=.claude/skills/xvision/SKILL.md
CLI_REF=.claude/skills/xvision/references/cli.md
WRAPPER=scripts/xvn-remote.py

grep -q '## For Agents' "$README"
grep -q '.claude/skills/xvision/SKILL.md' "$README"
grep -q 'scripts/xvn-remote.py' "$README"
grep -q 'Remote CLI over Tailscale' "$CLI_REF"
grep -q 'xvn eval run' "$CLI_REF"
grep -q 'scripts/xvn-remote.py exec' "$SKILL"
grep -q 'Tailscale-served remote CLI surface' "$SKILL"
grep -q 'Drive xvn over the remote CLI API' "$WRAPPER"
