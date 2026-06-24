#!/usr/bin/env bash
# Wake-and-deploy: watches open PRs until they all merge, then fast-forwards
# the main checkout and signals readiness. The actual build + deploy is left
# for a human or LLM to trigger so edge cases (worktree conflicts, disk space,
# stale containers) can be healed interactively.
#
# Usage:
#   scripts/watch-prs-and-deploy.sh [--push user@host] [--pr-timeout-seconds N]
#   # Default: polls all open PRs for latentwill/xvision, target root@100.120.48.1
#
# Once all PRs merge, this script prints a deploy-ready summary and exits.
# To then deploy:  scripts/deploy-image.sh --push <host>

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ── Config ────────────────────────────────────────────────────────────────────
PUSH_HOST="${PUSH_HOST:-root@100.120.48.1}"
POLL_INTERVAL="${POLL_INTERVAL:-30}"         # seconds between PR status checks
PR_TIMEOUT="${PR_TIMEOUT:-7200}"             # max seconds to wait (2h default)
REPO="latentwill/xvision"

# ── Parse args ────────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --push) PUSH_HOST="$2"; shift 2 ;;
    --poll-interval) POLL_INTERVAL="$2"; shift 2 ;;
    --pr-timeout) PR_TIMEOUT="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--push user@host] [--poll-interval N] [--pr-timeout N]"
      echo "Watches open PRs for $REPO until all merge, then signals readiness."
      echo "Defaults: target=$PUSH_HOST, poll=${POLL_INTERVAL}s, timeout=${PR_TIMEOUT}s"
      exit 0 ;;
    *)
      echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

# ── Preflight ─────────────────────────────────────────────────────────────────
if ! command -v gh >/dev/null 2>&1; then
  echo "ERROR: gh (GitHub CLI) not found on PATH. Install: brew install gh" >&2
  exit 1
fi

if ! gh auth status >/dev/null 2>&1; then
  echo "ERROR: gh not authenticated. Run: gh auth login" >&2
  exit 1
fi

echo "==> Wake target: $PUSH_HOST (deploy NOT automatic — run deploy-image.sh after wake)"
echo "==> Poll interval: ${POLL_INTERVAL}s, timeout: ${PR_TIMEOUT}s"
echo ""

# ── Poll loop ─────────────────────────────────────────────────────────────────
START_TIME="$(date +%s)"

while true; do
  ELAPSED=$(( $(date +%s) - START_TIME ))
  if (( ELAPSED > PR_TIMEOUT )); then
    echo "==> TIMEOUT: waited ${PR_TIMEOUT}s. PRs still open:" >&2
    gh pr list --repo "$REPO" --state open --json number,title --jq '.[] | "    #\(.number) \(.title)"' >&2
    exit 1
  fi

  echo "--- $(date '+%H:%M:%S') Checking PRs (+${ELAPSED}s) ---"

  OPEN_COUNT="$(gh pr list --repo "$REPO" --state open --json number --jq 'length')"

  if [[ "$OPEN_COUNT" == "0" ]]; then
    echo ""
# ── Checkout main + pull ──────────────────────────────────────────────────────
echo "==> Fetching origin/main..."
git fetch origin main --quiet
# If origin/main is already checked out in a worktree, use that worktree.
# Otherwise, force-checkout main in the current repo.
EXISTING_MAIN_WT="$(git worktree list | awk '$3 == "[main]" {print $1; exit}')"
if [[ -n "$EXISTING_MAIN_WT" ]]; then
  echo "==> Using existing main worktree: $EXISTING_MAIN_WT"
  cd "$EXISTING_MAIN_WT"
  echo "==> Fast-forwarding to origin/main..."
  git fetch origin main --quiet
  git merge --ff-only origin/main
  echo "    HEAD: $(git log -1 --format='%h %s')"
  # The deploy script needs to run from the repo root but with main checked out.
  # We'll run it from the main worktree — it only uses git for sha tagging.
  DEPLOY_CWD="$EXISTING_MAIN_WT"
else
  echo "==> Checking out origin/main..."
  git checkout --force -B main origin/main
  echo "    HEAD: $(git log -1 --format='%h %s')"
  DEPLOY_CWD="$REPO_ROOT"
fi
  fi

  echo "    $OPEN_COUNT open PR(s):"
  gh pr list --repo "$REPO" --state open --json number,title \
    --jq '.[] | "      #\(.number)  \(.title)"'

  echo "    Sleeping ${POLL_INTERVAL}s..."
  sleep "$POLL_INTERVAL"
done

# ── Checkout main + pull ──────────────────────────────────────────────────────
echo "==> Fetching origin/main..."
git fetch origin main --quiet
# If origin/main is already checked out in a worktree, use that worktree.
EXISTING_MAIN_WT="$(git worktree list | awk '$3 == "[main]" {print $1; exit}')"
if [[ -n "$EXISTING_MAIN_WT" ]]; then
  echo "==> Using existing main worktree: $EXISTING_MAIN_WT"
  cd "$EXISTING_MAIN_WT"
  echo "==> Fast-forwarding to origin/main..."
  git fetch origin main --quiet
  git merge --ff-only origin/main
  echo "    HEAD: $(git log -1 --format='%h %s')"
  DEPLOY_CWD="$EXISTING_MAIN_WT"
else
  echo "==> Checking out origin/main..."
  git checkout --force -B main origin/main
  echo "    HEAD: $(git log -1 --format='%h %s')"
  DEPLOY_CWD="$REPO_ROOT"
fi

# ── Deploy-ready summary ────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  All PRs merged. Main is up to date. Ready to deploy.       ║"
echo "╠══════════════════════════════════════════════════════════════╣"
echo "║  Commit:  $(git -C "$DEPLOY_CWD" log -1 --format='%h %s')"
echo "║  Target:  $PUSH_HOST"
echo "║  Deploy:  $DEPLOY_CWD/scripts/deploy-image.sh --push $PUSH_HOST"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Run the deploy command above, or let your LLM handle it."
