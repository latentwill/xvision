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
#   scripts/deploy-image.sh --no-cleanup             # keep old images post-deploy
#   scripts/deploy-image.sh --keep-hours 168         # prune window (default 72h)
#
# Post-deploy cleanup: by default this script prunes images older than 72h
# after a successful deploy (locally for build-only, on the target host for
# --push). Each xvision image is ~1.16GB and a 38GB Hetzner box fills up in
# 4-5 deploys without pruning — the symptom is an xvn-app crash loop on
# "database or disk is full" because sqlite can't open its WAL. Disable
# with --no-cleanup if you need an old image around for rollback.
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
# Default: prune images older than CLEANUP_KEEP_HOURS after a successful
# deploy. 38GB Hetzner boxes fill fast with 1.16GB xvision images; one
# missed prune cycle and the next deploy crash-loops on a "database or
# disk is full" sqlite error (xvn-app, 2026-05-19). Skip with --no-cleanup.
CLEANUP=1
CLEANUP_KEEP_HOURS=72

while [[ $# -gt 0 ]]; do
  case "$1" in
    --push)            PUSH_HOST="$2"; shift 2 ;;
    --tag)             TAG="$2"; shift 2 ;;
    --platform)        PLATFORM="$2"; shift 2 ;;
    --with-identity)   WITH_IDENTITY=1; shift ;;
    --no-cleanup)      CLEANUP=0; shift ;;
    --keep-hours)      CLEANUP_KEEP_HOURS="$2"; shift 2 ;;
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
  if [[ "$CLEANUP" -eq 1 ]]; then
    echo "==> Local cleanup (images older than ${CLEANUP_KEEP_HOURS}h)"
    docker image prune -af --filter "until=${CLEANUP_KEEP_HOURS}h" 2>&1 | tail -2 || true
    df -h / 2>/dev/null | tail -1 || true
  fi
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

if [[ "$CLEANUP" -eq 1 ]]; then
  # Remote cleanup. Without this the deploy host fills up: each build
  # produces a ~1.16GB image, the previous :deploy-latest gets orphaned,
  # and a 38GB box hits 100% in 4-5 deploys. When sqlite can't open its
  # WAL ("database or disk is full") xvn-app crash-loops on startup —
  # exactly what bit us on the 2026-05-19 deploy of sha-365dc14.
  #
  # Skip with --no-cleanup if you need to keep older images around
  # (e.g. for a rollback). Tune the window with --keep-hours N.
  echo "==> Remote cleanup on $PUSH_HOST (images older than ${CLEANUP_KEEP_HOURS}h)"
  ssh "$PUSH_HOST" "docker image prune -af --filter 'until=${CLEANUP_KEEP_HOURS}h' 2>&1 | tail -2 || true; df -h / 2>/dev/null | tail -1 || true"
fi

cat <<EOF

Loaded $TAG on $PUSH_HOST and tagged xvision:deploy-latest.

Next on the server:
  ssh $PUSH_HOST
  # Recreate every stack that points at xvision:deploy-latest.
  # Example compose stacks:
  #   cd /root/deploy/stacks/xvn && docker compose up -d --force-recreate xvn
  #   cd /root/deploy/stacks/xvnej && docker compose up -d --force-recreate xvnej
  # The --force-recreate is needed because the app shares a netns with
  # the ts-* sidecar; without it the container can re-attach to a stale
  # netns and fail with "no such file or directory" on /proc/.../ns/net.
  # For Coolify-managed apps, trigger a redeploy there instead.
EOF
