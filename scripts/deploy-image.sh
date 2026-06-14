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
#
# Environment (marketplace build-time config, baked into the SPA):
#   VITE_MARKETPLACE_NETWORK=mainnet   # chain for the buy path (default: sepolia)
#   VITE_MARKETPLACE_SUBGRAPH_URL=...  # Goldsky subgraph that Browse reads.
#                                      # Forwarded to the build when set (even to
#                                      # an empty string). On a mainnet build it
#                                      # defaults to "" so Browse reads the
#                                      # backend indexer instead of the baked
#                                      # testnet subgraph (which would otherwise
#                                      # show Sepolia listings on a mainnet site).

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

# Build provenance baked into the image (ARG GIT_SHA / BUILD_TIME in
# Dockerfile.deploy → ENV XVN_GIT_SHA / XVN_BUILT_AT + OCI labels →
# GET /api/version). Full SHA carries the dirty marker so a build off an
# uncommitted tree is never mistaken for a clean commit.
FULL_SHA="$(git rev-parse HEAD 2>/dev/null || echo "nogit")${DIRTY}"
BUILD_TIME="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

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

# --- Marketplace build-time config passthrough --------------------------------
# The Dockerfile bakes the testnet Goldsky subgraph URL as its ARG default, and
# the data-source selector uses the subgraph whenever that URL is non-empty. So
# a mainnet image built without overriding the URL queries the Sepolia subgraph
# and shows testnet listings in Browse. Forward an explicit
# VITE_MARKETPLACE_SUBGRAPH_URL when the operator sets one (honoring an explicit
# empty value, which routes Browse to the backend indexer), and default it to
# empty on a mainnet build so the testnet subgraph is never shipped to mainnet.
MARKETPLACE_NETWORK="${VITE_MARKETPLACE_NETWORK:-sepolia}"
MARKETPLACE_BUILD_ARGS=()
if [[ -z "${VITE_MARKETPLACE_SUBGRAPH_URL+set}" && "$MARKETPLACE_NETWORK" == "mainnet" ]]; then
  VITE_MARKETPLACE_SUBGRAPH_URL=""
  echo "==> mainnet build: defaulting VITE_MARKETPLACE_SUBGRAPH_URL='' (Browse uses the backend indexer, not the baked testnet subgraph)"
fi
if [[ -n "${VITE_MARKETPLACE_SUBGRAPH_URL+set}" ]]; then
  MARKETPLACE_BUILD_ARGS+=(--build-arg "VITE_MARKETPLACE_SUBGRAPH_URL=${VITE_MARKETPLACE_SUBGRAPH_URL}")
  echo "==> Marketplace subgraph: VITE_MARKETPLACE_SUBGRAPH_URL='${VITE_MARKETPLACE_SUBGRAPH_URL:-<empty>}'"
fi

echo "==> Building $TAG (WITH_IDENTITY=$WITH_IDENTITY, platform=$PLATFORM, network=$MARKETPLACE_NETWORK)"
echo "==> Provenance: GIT_SHA=$FULL_SHA  BUILD_TIME=$BUILD_TIME"
# `--load` so the image lands in the local daemon (default for single-platform
# builds, but explicit is better when buildx is the active builder).
DOCKER_BUILDKIT=1 docker buildx build \
  -f Dockerfile.deploy \
  --platform "$PLATFORM" \
  --build-arg "WITH_IDENTITY=${WITH_IDENTITY}" \
  --build-arg "GIT_SHA=${FULL_SHA}" \
  --build-arg "BUILD_TIME=${BUILD_TIME}" \
  --build-arg "VITE_MARKETPLACE_NETWORK=${MARKETPLACE_NETWORK}" \
  ${MARKETPLACE_BUILD_ARGS[@]+"${MARKETPLACE_BUILD_ARGS[@]}"} \
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

# --- Remote disk preflight ---------------------------------------------------
# Refuses (or warns) if the target host's root partition is too full to safely
# receive the image. Driven by the 2026-05-20 incident: deploy succeeded at
# image-load but xvn-app entered a restart loop because `/` was at 100% — SQLite
# couldn't write the migration journal. The 38 GB Hetzner host fills with old
# image layers + leftover Cargo `target/` debris in worktrees and silently
# bricks deploys.
#
# Threshold defaults: refuse at 95%, warn at 85%. Override with
# XVN_DEPLOY_DISK_REFUSE_PCT / XVN_DEPLOY_DISK_WARN_PCT. Bypass refusal with
# XVN_DEPLOY_SKIP_DISK_CHECK=1.
REFUSE_PCT="${XVN_DEPLOY_DISK_REFUSE_PCT:-95}"
WARN_PCT="${XVN_DEPLOY_DISK_WARN_PCT:-85}"
if [[ "${XVN_DEPLOY_SKIP_DISK_CHECK:-0}" != "1" ]]; then
  REMOTE_DF="$(ssh "$PUSH_HOST" "df -P / | awk 'NR==2 {print \$5,\$4}'" 2>/dev/null || true)"
  if [[ -n "$REMOTE_DF" ]]; then
    REMOTE_USED_PCT="${REMOTE_DF%%%*}"
    REMOTE_USED_PCT="${REMOTE_USED_PCT// /}"
    REMOTE_AVAIL_KB="$(awk '{print $2}' <<<"$REMOTE_DF")"
    REMOTE_AVAIL_HUMAN="$(awk -v k="$REMOTE_AVAIL_KB" 'BEGIN{
      split("K M G T",u); v=k;i=1;
      while (v>=1024 && i<4){v/=1024;i++}
      printf("%.1f%sB", v, u[i])
    }')"
    echo "==> Remote disk: ${REMOTE_USED_PCT}% used on / (${REMOTE_AVAIL_HUMAN} free)"
    if [[ "$REMOTE_USED_PCT" =~ ^[0-9]+$ ]] && (( REMOTE_USED_PCT >= REFUSE_PCT )); then
      echo "==> REFUSING: ${REMOTE_USED_PCT}% >= ${REFUSE_PCT}% threshold." >&2
      echo "    Image will load but containers using SQLite will hit \"database or disk is full\"." >&2
      echo "    Free space on \$PUSH_HOST then retry. Common reclaim targets:" >&2
      echo "      journalctl --vacuum-size=200M" >&2
      echo "      docker image prune -f                       # dangling only" >&2
      echo "      docker images xvision --format '{{.Tag}}'   # find old deploy-<sha> tags" >&2
      echo "      find /root/deploy/xvision/.worktrees -name target -type d -prune -exec rm -rf {} +" >&2
      echo "    Bypass: XVN_DEPLOY_SKIP_DISK_CHECK=1 $0 ..." >&2
      exit 1
    elif [[ "$REMOTE_USED_PCT" =~ ^[0-9]+$ ]] && (( REMOTE_USED_PCT >= WARN_PCT )); then
      echo "==> WARNING: ${REMOTE_USED_PCT}% >= ${WARN_PCT}% — deploy will likely succeed but disk is getting tight." >&2
      echo "    Sleeping 5s so you can Ctrl-C..." >&2
      sleep 5
    fi
  fi
fi

# Capture the prior :deploy-latest image id (if any) so post-load cleanup can
# drop its now-stale sha tag safely.
PRIOR_LATEST_ID="$(ssh "$PUSH_HOST" "docker image inspect xvision:deploy-latest --format '{{.Id}}' 2>/dev/null" || true)"

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

NEW_LATEST_ID="$(ssh "$PUSH_HOST" "docker image inspect xvision:deploy-latest --format '{{.Id}}'")"

# --- Post-load cleanup -------------------------------------------------------
# Drop the prior :deploy-latest's sha tag IFF (a) it's a different image than
# what we just loaded, and (b) no container still references that image. Keeps
# the host from accumulating 1.1 GB per deploy. Other xvision: tags (including
# what xvnej-app is pinned to, ghcr.io/* mirrors, or anything actively in use)
# are untouched. Bypass with XVN_DEPLOY_SKIP_CLEANUP=1.
if [[ "${XVN_DEPLOY_SKIP_CLEANUP:-0}" != "1" \
      && -n "$PRIOR_LATEST_ID" \
      && "$PRIOR_LATEST_ID" != "$NEW_LATEST_ID" ]]; then
  echo "==> Post-load cleanup: prior :deploy-latest was $PRIOR_LATEST_ID; checking if reclaimable"
  ssh "$PUSH_HOST" "
    OLD_ID='$PRIOR_LATEST_ID'
    # Any container (running or stopped) still on the old image?
    if docker ps -a --format '{{.ImageID}} {{.Image}}' | awk '{print \$1}' | grep -Fq \"\$OLD_ID\"; then
      echo '    keeping prior image — still referenced by a container'
    else
      # List xvision-namespace tags pointing at the old image id and drop them.
      # docker emits the 12-char short id in column 2; OLD_ID is the full
      # sha256:... form, so we test whether the short id is a substring of it.
      docker images --format '{{.Repository}}:{{.Tag}} {{.ID}}' \
        | awk -v id=\"\$OLD_ID\" '\$1 ~ /^xvision:/ && index(id, \$2) > 0 { print \$1 }' \
        | while read t; do
            [ \"\$t\" = 'xvision:deploy-latest' ] && continue
            [ \"\$t\" = '$TAG' ] && continue
            echo \"    docker image rm \$t\"
            docker image rm \"\$t\" 2>&1 | tail -1 || true
          done
    fi
    # Sweep dangling layers freed by any of the above.
    docker image prune -f 2>&1 | tail -1
  "
fi

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
