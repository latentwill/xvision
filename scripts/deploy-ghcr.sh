#!/usr/bin/env bash
# Canonical GHCR deployment path for xvision.
# - Triggers `.github/workflows/docker.yml` via workflow_dispatch.
# - Uses Dockerfile.deploy (SPA baked in) by default.
# - Watches the run and fails fast if CI fails.
#
# Hard rule: use this on build/control hosts; do NOT run cargo on deploy hosts.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if ! command -v gh >/dev/null 2>&1; then
  echo "deploy-ghcr: gh CLI not found on PATH" >&2
  exit 1
fi

DOCKERFILE="Dockerfile.deploy"
BUILD_IDENTITY="false"
BUILD_PROFILE="release"
REF="main"
WATCH=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ref) REF="$2"; shift 2 ;;
    --dockerfile) DOCKERFILE="$2"; shift 2 ;;
    --build-identity) BUILD_IDENTITY="$2"; shift 2 ;;
    --build-profile) BUILD_PROFILE="$2"; shift 2 ;;
    --no-watch) WATCH=0; shift ;;
    -h|--help)
      cat <<'EOF'
Usage:
  scripts/deploy-ghcr.sh [options]

Options:
  --ref <branch>             Git ref for workflow_dispatch (default: main)
  --dockerfile <path>        Dockerfile to build (default: Dockerfile.deploy)
  --build-identity <bool>    true|false (default: false)
  --build-profile <profile>  release|staging (default: release)
  --no-watch                 Dispatch only, do not watch run

Examples:
  scripts/deploy-ghcr.sh
  scripts/deploy-ghcr.sh --build-profile staging
  scripts/deploy-ghcr.sh --ref main --build-identity true
EOF
      exit 0
      ;;
    *)
      echo "deploy-ghcr: unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ "$DOCKERFILE" != "Dockerfile.deploy" && "$DOCKERFILE" != "Dockerfile" ]]; then
  echo "deploy-ghcr: --dockerfile must be Dockerfile.deploy or Dockerfile" >&2
  exit 2
fi

if [[ "$BUILD_IDENTITY" != "true" && "$BUILD_IDENTITY" != "false" ]]; then
  echo "deploy-ghcr: --build-identity must be true or false" >&2
  exit 2
fi

if [[ "$BUILD_PROFILE" != "release" && "$BUILD_PROFILE" != "staging" ]]; then
  echo "deploy-ghcr: --build-profile must be release or staging" >&2
  exit 2
fi

echo "==> Trigger GHCR workflow"
echo "    ref=$REF dockerfile=$DOCKERFILE build_identity=$BUILD_IDENTITY build_profile=$BUILD_PROFILE"

gh workflow run docker.yml \
  --ref "$REF" \
  -f dockerfile="$DOCKERFILE" \
  -f build_identity="$BUILD_IDENTITY" \
  -f build_profile="$BUILD_PROFILE"

RUN_ID="$(
  gh run list --workflow docker.yml --limit 1 --json databaseId --jq '.[0].databaseId'
)"

if [[ -z "${RUN_ID:-}" ]]; then
  echo "deploy-ghcr: unable to resolve run id" >&2
  exit 1
fi

RUN_URL="https://github.com/latentwill/xvision/actions/runs/${RUN_ID}"
echo "==> Run queued: ${RUN_ID}"
echo "    ${RUN_URL}"

if [[ "$WATCH" -eq 1 ]]; then
  gh run watch "$RUN_ID" --interval 10 --exit-status
fi

echo "==> Final status"
gh run view "$RUN_ID" --json status,conclusion,url
