#!/usr/bin/env bash
# docs-freshness-lint.sh — keep `crates/xvision-dashboard/wiki/` honest.
#
# Two checks. Both run by default; you can opt into one with a flag.
#
#   --check-staleness         — every [[page]] in wiki/index.toml has a
#                               `last_reviewed: YYYY-MM-DD` value AND the
#                               date is within 90 days of `date +%Y-%m-%d`
#                               (or the date supplied via --head-date).
#
#   --check-recent-verbs      — if a new top-level verb file lands under
#                               crates/xvision-cli/src/commands/, then
#                               crates/xvision-dashboard/wiki/cli-reference.md
#                               must be modified in the same diff.
#
# Either fires when violated → exit 1. Clean tree → exit 0.
#
# Source: team/contracts/docs-freshness-staleness-guard.md.
# Portable: no GNU-specific date flags, no gawk-only features.
# Tested on macOS BSD utils + Ubuntu GNU utils.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$REPO_ROOT/crates/xvision-dashboard/wiki/index.toml"

MAX_AGE_DAYS=${DOCS_MAX_AGE_DAYS:-90}

# ── Helpers ───────────────────────────────────────────────────────────────

# date_diff_days <today YYYY-MM-DD> <past YYYY-MM-DD>
# Returns the integer day delta. Pure-python so we don't depend on
# GNU date's -d flag (BSD date doesn't have it).
date_diff_days() {
  python3 - "$1" "$2" <<'PY'
import sys
from datetime import date
def parse(s):
    y, m, d = s.split("-")
    return date(int(y), int(m), int(d))
today = parse(sys.argv[1])
past = parse(sys.argv[2])
delta = (today - past).days
print(delta)
PY
}

usage() {
  sed -n '2,20p' "$0"
}

# ── Arg parsing ───────────────────────────────────────────────────────────

check_staleness=0
check_recent_verbs=0
head_date=""

while [ $# -gt 0 ]; do
  case "$1" in
    --check-staleness) check_staleness=1 ;;
    --check-recent-verbs) check_recent_verbs=1 ;;
    --head-date) head_date="${2:-}"; shift ;;
    --help|-h) usage; exit 0 ;;
    *)
      echo "docs-freshness-lint: unknown arg '$1' (try --help)" >&2
      exit 2
      ;;
  esac
  shift
done

if [ "$check_staleness" -eq 0 ] && [ "$check_recent_verbs" -eq 0 ]; then
  check_staleness=1
  check_recent_verbs=1
fi

if [ -n "$head_date" ]; then
  today="$head_date"
else
  today="$(date +%Y-%m-%d)"
fi

fail=0

# ── 1. Staleness check ────────────────────────────────────────────────────

check_one_page() {
  local slug="$1"
  local review="$2"
  if [ -z "$review" ]; then
    echo "docs-freshness-lint: page slug='$slug' missing last_reviewed in $MANIFEST" >&2
    fail=1
    return
  fi
  local age
  age="$(date_diff_days "$today" "$review")"
  if [ "$age" -gt "$MAX_AGE_DAYS" ]; then
    echo "docs-freshness-lint: page slug='$slug' is $age days old (last_reviewed=$review, cap=$MAX_AGE_DAYS)" >&2
    fail=1
  fi
}

if [ "$check_staleness" -eq 1 ]; then
  if [ ! -f "$MANIFEST" ]; then
    echo "docs-freshness-lint: manifest not found: $MANIFEST" >&2
    exit 1
  fi
  # The manifest shape is stable: each `[[page]]` block has a
  # `slug = "..."` and a `last_reviewed = "..."` within the same block.
  current_slug=""
  current_review=""
  while IFS= read -r line; do
    case "$line" in
      "[[page]]")
        if [ -n "$current_slug" ]; then
          check_one_page "$current_slug" "$current_review"
        fi
        current_slug=""
        current_review=""
        ;;
      "slug = "*)
        current_slug="$(echo "$line" | sed -E 's/^slug = "([^"]+)"/\1/')"
        ;;
      "last_reviewed = "*)
        current_review="$(echo "$line" | sed -E 's/^last_reviewed = "([^"]+)"/\1/')"
        ;;
    esac
  done < "$MANIFEST"
  # Flush the trailing block.
  if [ -n "$current_slug" ]; then
    check_one_page "$current_slug" "$current_review"
  fi
fi

# ── 2. New-verb sync check ────────────────────────────────────────────────

if [ "$check_recent_verbs" -eq 1 ]; then
  base_ref="${GITHUB_BASE_REF:-main}"
  base_remote="origin/$base_ref"
  if git -C "$REPO_ROOT" rev-parse "$base_remote" >/dev/null 2>&1; then
    # `--diff-filter=A` = added files only. Top-level verbs are
    # commands/<name>.rs; commands/<group>/<sub>.rs are sub-leaves of
    # an existing group and don't trigger.
    new_verbs="$(git -C "$REPO_ROOT" diff --name-status --diff-filter=A "$base_remote"...HEAD \
      | awk -v dir="crates/xvision-cli/src/commands/" '
          {
            if ($2 ~ "^"dir"[^/]+\\.rs$") {
              print $2
            }
          }
        ')" || new_verbs=""

    if [ -n "$new_verbs" ]; then
      cli_ref_path="crates/xvision-dashboard/wiki/cli-reference.md"
      if ! git -C "$REPO_ROOT" diff --name-only "$base_remote"...HEAD \
        | grep -qx "$cli_ref_path"; then
        echo "docs-freshness-lint: new top-level CLI verb(s) added without touching $cli_ref_path:" >&2
        while IFS= read -r v; do
          [ -n "$v" ] && echo "  - $v" >&2
        done <<< "$new_verbs"
        echo "  Update $cli_ref_path in the same PR (and bump its last_reviewed)." >&2
        fail=1
      fi
    fi
  fi
  # Outside CI (no origin/main) → silently skip.
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "docs-freshness-lint: ok"
