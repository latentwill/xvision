#!/usr/bin/env bash
# PreToolUse hook on Bash: refuse to run `cargo*` or `docker build` on
# xvision deploy hosts. Enforced via the XVISION_DEPLOY_HOST=1 marker
# (set in the deploy host's shell profile). On dev workstations the marker
# is absent and this hook is a no-op.
#
# See CLAUDE.md "Deployment guardrails (hard rules)".

set -u

# Marker absent → not a deploy host, allow everything.
[ "${XVISION_DEPLOY_HOST:-}" = "1" ] || exit 0

input="$(cat)"
cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
[ -n "$cmd" ] || exit 0

deny() {
  local reason="$1"
  jq -n --arg r "$reason" '{
    "hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "permissionDecision": "deny",
      "permissionDecisionReason": $r
    }
  }'
  exit 0
}

# Strip common prefixes (cd /path && , env VAR=val , sudo ).
stripped="$cmd"

case "$stripped" in
  *cargo\ *|*cargo)
    deny "Refused: \`cargo\` must not run on this host (XVISION_DEPLOY_HOST=1). Builds happen on the local build host (scripts/deploy-image.sh) or in GHCR. See CLAUDE.md."
    ;;
  *docker\ build*|*docker\ buildx*|*"docker compose build"*|*docker-compose\ build*)
    deny "Refused: docker image builds must not run on deploy hosts (OOM risk on small VPS). Build locally with scripts/deploy-image.sh --push, then docker pull/load on this host."
    ;;
  *"cargo "*)
    deny "Refused: \`cargo\` must not run on this host (XVISION_DEPLOY_HOST=1). See CLAUDE.md deployment guardrails."
    ;;
esac

exit 0
