#!/usr/bin/env bash
# check_agent_docs.sh — keep the agent-facing docs/skills honest.
#
# Verifies the "For Agents" entrypoint in the README, the split operator/dev
# skills (the single `xvision` skill was split into `xvision-cli`,
# `xvision-dev`, and `xvision-cli-qa`), the remote-CLI helper, and the
# chat-rail / optimizer / diagnostics surfaces shipped in the 2026-05-24 wave.
#
# Exits non-zero with a per-check message on the first failure.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

README=README.md
CLI_SKILL=.claude/skills/xvision-cli/SKILL.md
DEV_SKILL=.claude/skills/xvision-dev/SKILL.md
QA_SKILL=.claude/skills/xvision-cli-qa/SKILL.md
CLI_REF=.claude/skills/xvision-cli/references/cli.md
WRAPPER=scripts/xvn-remote.py
WIKI=crates/xvision-dashboard/wiki

fail=0

# need <file> <pattern> <human description>
need() {
  local file="$1" pat="$2" desc="$3"
  if [ ! -f "$file" ]; then
    echo "check_agent_docs: missing file: $file" >&2
    fail=1
    return
  fi
  if ! grep -q "$pat" "$file"; then
    echo "check_agent_docs: $file is missing $desc (grep '$pat')" >&2
    fail=1
  fi
}

# ── README entrypoint ───────────────────────────────────────────────────────
need "$README" '## For Agents'                            'the "For Agents" section'
need "$README" '.claude/skills/xvision-cli/SKILL.md'      'a pointer to the xvision-cli skill'
need "$README" '.claude/skills/xvision-dev/SKILL.md'      'a pointer to the xvision-dev skill'
need "$README" 'scripts/xvn-remote.py'                    'the remote-CLI helper reference'

# ── Split skills exist and carry their anchors ──────────────────────────────
need "$CLI_REF"  'Remote CLI over Tailscale'              'the remote-CLI section'
need "$CLI_REF"  'xvn eval run'                           'the eval-run example'
need "$CLI_SKILL" 'scripts/xvn-remote.py exec'            'the remote-CLI exec helper'
need "$CLI_SKILL" 'Tailscale-served remote CLI surface'   'the remote-CLI surface note'
need "$WRAPPER"  'Drive xvn over the remote CLI API'      'its docstring banner'

# ── 2026-05-24 wave: chat rail / optimizer / diagnostics ────────────────────
# Operator/usage skill must document the new driving surfaces.
need "$CLI_SKILL" 'xvn optimize'                          'the xvn optimize verb'
need "$CLI_SKILL" 'strategy diagnostics'                  'capability diagnostics'
need "$CLI_SKILL" '/api/chat-rail/sessions/:id/stream'    'the unified chat-rail stream'

# QA skill must carry the wave watch-fors.
need "$QA_SKILL" 'xvn optimize'                           'the optimizer QA section'
need "$QA_SKILL" 'research mode'                          'the Research/Act denial watch-for'
need "$QA_SKILL" 'holdout'                                'the accept-without-holdout watch-for'

# Dev skill must record the offline-only DSPy invariant.
need "$DEV_SKILL" 'xvision-dspy'                          'the xvision-dspy crate'
need "$DEV_SKILL" 'DSPy-free'                             'the engine/dashboard-stays-dspy-free invariant'

# Baked wiki pages for the wave must exist and be registered in the manifest.
need "$WIKI/cli-reference.md" 'xvn optimize'              'the optimize CLI reference'
need "$WIKI/agents.md"        'Improve this agent'        'the Improve-this-agent flow'
need "$WIKI/optimizer.md"     'Offline-only invariant'    'the offline-only invariant'
need "$WIKI/driving-xvn-as-an-agent.md" 'Research / Act mode' 'the Research/Act section'
need "$WIKI/index.toml"       'slug = "optimizer"'        'the optimizer page registration'

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "check_agent_docs: ok"
