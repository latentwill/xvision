#!/usr/bin/env bash
# Path A deploy flow: build the dashboard-baked xvision image locally, then
# ship it to a remote host over SSH (no GHCR / no GitHub Actions credits).
# After the image lands, any Docker Compose or Coolify service that consumes
# `xvision:deploy-latest` must be recreated/redeployed so the running container
# actually switches to the new image.
#
# Avoids the OOM trap when the deploy host is small (4 GB RAM / 38 GB disk):
# the Rust + Vite build runs on the build host, and only the ~150 MB runtime
# image gets transferred.
#
# Usage:
#   scripts/deploy-image.sh                          # build only, tag xvision:deploy-<sha>
#   scripts/deploy-image.sh --push root@host         # build + transfer + docker load
#   scripts/deploy-image.sh --tag xvision:custom     # override image tag
#   scripts/deploy-image.sh --with-identity          # include xvision-identity (Mantle)
#   scripts/deploy-image.sh --platform linux/arm64   # build for arm64 servers
#   scripts/deploy-image.sh --push root@host --tag xvision:deploy-rc1
#
# Defaults to --platform linux/amd64 since most VPS hosts (DO, Linode, Vultr,
# Hetzner x86, Coolify default) are x86_64. Override for Graviton / Oracle
# ARM / Hetzner ARM. Cross-arch builds on Apple Silicon run through qemu and
# take noticeably longer (~2× wall time on a Rust workspace this size).
#
# Requirements (build host):
#   - docker with BuildKit support
#   - git (the working tree's HEAD short-sha is used as the default tag)
#   - ssh access to the target host (--push only)
#
# The target host needs nothing besides docker.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PUSH_HOST=""
TAG=""
WITH_IDENTITY=0
PLATFORM="linux/amd64"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --push)          PUSH_HOST="$2"; shift 2 ;;
    --tag)           TAG="$2"; shift 2 ;;
    --platform)      PLATFORM="$2"; shift 2 ;;
    --with-identity) WITH_IDENTITY=1; shift ;;
    -h|--help)
      sed -n '2,/^set -euo pipefail$/p' "$0" | sed 's/^# \?//;/^set/d'
      exit 0 ;;
    *)
      echo "deploy-image: unknown arg: $1" >&2
      exit 2 ;;
  esac
done

if ! command -v docker >/dev/null 2>&1; then
  echo "deploy-image: docker not on PATH" >&2
  exit 1
fi

SHA="$(git rev-parse --short HEAD 2>/dev/null || echo "nogit")"
DIRTY=""
if ! git diff --quiet 2>/dev/null || ! git diff --cached --quiet 2>/dev/null; then
  DIRTY="-dirty"
fi
[[ -z "$TAG" ]] && TAG="xvision:deploy-${SHA}${DIRTY}"

# --- Branch sanity preflight ---------------------------------------------------
# This block surfaces what's about to be built so deploys can't silently ship
# from a branch that's parallel to main (the failure mode that produced the
# 2026-05-19 "deployed image had none of the merged fixes" incident — the build
# host was on a conductor coordination branch 634 commits adrift from
# origin/main).
#
# Non-blocking: prints a loud warning and continues. Set XVN_DEPLOY_QUIET=1 to
# silence (for CI / scripted retries that have already accepted the risk).
if [[ "${XVN_DEPLOY_QUIET:-0}" != "1" ]] && command -v git >/dev/null 2>&1; then
  BRANCH="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "?")"
  HEAD_MSG="$(git log -1 --format='%s' 2>/dev/null | head -c 80)"
  echo "==> Preflight: branch=$BRANCH  HEAD=$SHA${DIRTY:+ ($DIRTY)}  msg=\"$HEAD_MSG\""
  if git rev-parse --verify origin/main >/dev/null 2>&1; then
    AHEAD_MAIN="$(git rev-list --count origin/main..HEAD 2>/dev/null || echo "?")"
    BEHIND_MAIN="$(git rev-list --count HEAD..origin/main 2>/dev/null || echo "?")"
    MERGE_BASE="$(git merge-base HEAD origin/main 2>/dev/null || echo "")"
    if [[ -z "$MERGE_BASE" ]]; then
      echo "==> WARNING: HEAD shares NO common ancestor with origin/main." >&2
      echo "    This is almost always wrong for a deploy — you'll ship a parallel" >&2
      echo "    history that's missing every PR merged into main." >&2
      echo "    Override with XVN_DEPLOY_QUIET=1 if you really mean it." >&2
      echo "    Sleeping 8s so you can Ctrl-C..." >&2
      sleep 8
    elif [[ "$BEHIND_MAIN" =~ ^[0-9]+$ ]] && (( BEHIND_MAIN > 20 )); then
      echo "==> WARNING: HEAD is $BEHIND_MAIN commits behind origin/main." >&2
      echo "    Recently merged PRs will not be in the image. If that's not what" >&2
      echo "    you want, rebase onto origin/main first." >&2
      echo "    Sleeping 5s so you can Ctrl-C..." >&2
      sleep 5
    else
      echo "    (+$AHEAD_MAIN / -$BEHIND_MAIN vs origin/main — looks current)"
    fi
  fi
  if [[ -n "$DIRTY" ]]; then
    echo "==> Note: working tree is dirty; uncommitted changes will be baked into the image."
  fi
fi

echo "==> Building $TAG (WITH_IDENTITY=$WITH_IDENTITY, platform=$PLATFORM)"
# `--load` so the image lands in the local daemon (default for single-platform
# builds, but explicit is better when buildx is the active builder).
DOCKER_BUILDKIT=1 docker buildx build \
  -f Dockerfile.deploy \
  --platform "$PLATFORM" \
  --build-arg "WITH_IDENTITY=${WITH_IDENTITY}" \
  --load \
  -t "$TAG" \
  -t "xvision:deploy-latest" \
  .

echo "==> Image built:"
docker image ls "$TAG" --format '    {{.Repository}}:{{.Tag}}  {{.Size}}'

if [[ -z "$PUSH_HOST" ]]; then
  cat <<EOF

Built locally. Tags: $TAG, xvision:deploy-latest.

Quick smoke test:
  docker run --rm -p 8788:8788 -e XVN_AUTOMIGRATE=1 $TAG
  # then open http://localhost:8788

To ship to a server:
  $0 --push user@host
EOF
  exit 0
fi

echo "==> Saving + streaming to $PUSH_HOST"
# pv if available gives a progress bar; otherwise fall back to silent pipe.
if command -v pv >/dev/null 2>&1; then
  SIZE_BYTES="$(docker image inspect "$TAG" --format '{{.Size}}')"
  docker save "$TAG" | pv -s "$SIZE_BYTES" | gzip | ssh "$PUSH_HOST" "gunzip | docker load"
else
  docker save "$TAG" | gzip | ssh "$PUSH_HOST" "gunzip | docker load"
fi

# Tag :deploy-latest on the remote so compose files can pin a stable name.
ssh "$PUSH_HOST" "docker tag '$TAG' xvision:deploy-latest"

cat <<EOF

Loaded $TAG on $PUSH_HOST and tagged xvision:deploy-latest.

Next on the server:
  ssh $PUSH_HOST
  # Recreate every stack that points at xvision:deploy-latest.
  # Example compose stacks:
  #   cd /root/deploy/stacks/xvn && docker compose up -d
  #   cd /root/deploy/stacks/xvnej && docker compose up -d
  # For Coolify-managed apps, trigger a redeploy there instead.
EOF
