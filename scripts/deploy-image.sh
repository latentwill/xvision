#!/usr/bin/env bash
# Path A deploy flow: build the dashboard-baked xvision image locally, then
# ship it to a remote host over SSH (no GHCR / no GitHub Actions credits).
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
  cd /root/deploy/stacks/xvn
  # Make sure docker-compose.yml references image: xvision:deploy-latest
  docker compose up -d
  docker compose logs -f xvn
EOF
