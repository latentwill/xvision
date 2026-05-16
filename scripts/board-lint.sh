#!/usr/bin/env bash
# scripts/board-lint.sh — enforce execution-board consistency
#
# Checks:
#   1. Every active contract under team/contracts/ has frontmatter required fields.
#   2. Every active contract's branch exists on origin OR the contract is `status: ready|archived|merged|deferred`.
#   3. Every active contract's worktree directory exists OR the contract is `status: ready|archived|merged|deferred`.
#   4. team/status/<track>.md `phase:` (if present) is in the allowed vocabulary.
#   5. No two active contracts list the same allowed_path glob unless declared
#      multi-owner in team/OWNERSHIP.md or stacked in CONFLICT_ZONES.md.
#
# Run from the repo root. Exits non-zero on any violation.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

CONTRACTS_DIR="team/contracts"
STATUS_DIR="team/status"
OWNERSHIP_FILE="team/OWNERSHIP.md"
CONFLICT_FILE="team/CONFLICT_ZONES.md"

ALLOWED_STATUS_RE='^(ready|claimed|in-progress|pr-open|needs-rebase|merged|archived|blocked|deferred|scope-violation)$'
ALLOWED_LANE_RE='^(foundation|leaf|integration)$'

violations=0

bail() {
  echo "FAIL: $*" >&2
  violations=$((violations + 1))
}

ok() {
  echo "ok: $*"
}

if [ ! -d "$CONTRACTS_DIR" ]; then
  bail "$CONTRACTS_DIR missing"
  exit 1
fi

# 1. Frontmatter required fields per contract
shopt -s nullglob
contracts=()
for f in "$CONTRACTS_DIR"/*.md; do
  base="$(basename "$f")"
  [ "$base" = "_template.md" ] && continue
  contracts+=("$f")

  # Extract frontmatter (between first two `---` lines).
  fm="$(awk '/^---$/{c++; next} c==1{print} c==2{exit}' "$f")"

  for key in track lane wave worktree branch base status; do
    if ! grep -qE "^${key}:" <<<"$fm"; then
      bail "$f: missing frontmatter key '${key}'"
    fi
  done

  status="$(grep -E '^status:' <<<"$fm" | head -1 | awk '{print $2}')"
  lane="$(grep -E '^lane:' <<<"$fm" | head -1 | awk '{print $2}')"
  branch="$(grep -E '^branch:' <<<"$fm" | head -1 | awk '{print $2}')"
  worktree="$(grep -E '^worktree:' <<<"$fm" | head -1 | awk '{print $2}')"

  if ! [[ "$status" =~ $ALLOWED_STATUS_RE ]]; then
    bail "$f: status '$status' not in allowed vocabulary"
  fi
  if ! [[ "$lane" =~ $ALLOWED_LANE_RE ]]; then
    bail "$f: lane '$lane' not in allowed vocabulary"
  fi

  # 2. Branch existence on origin — only enforced for non-ready/non-archived
  case "$status" in
    ready|archived|merged|deferred) ;;
    *)
      if ! git ls-remote --exit-code --heads origin "$branch" >/dev/null 2>&1; then
        bail "$f: branch '$branch' missing on origin (status=$status)"
      fi
      ;;
  esac

  # 3. Worktree existence — same logic
  case "$status" in
    ready|archived|merged|deferred) ;;
    *)
      if [ ! -d "$worktree" ]; then
        bail "$f: worktree '$worktree' missing on disk (status=$status)"
      fi
      ;;
  esac
done

# 4. Status file phase vocabulary
if [ -d "$STATUS_DIR" ]; then
  for s in "$STATUS_DIR"/*.md; do
    [ -e "$s" ] || continue
    phase="$(awk -F': *' '/^phase:/{print $2; exit}' "$s")"
    if [ -n "$phase" ] && ! [[ "$phase" =~ $ALLOWED_STATUS_RE ]]; then
      bail "$s: phase '$phase' not in allowed vocabulary"
    fi
  done
fi

# 5. Overlap detection on allowed_paths (best-effort: exact-glob duplicates)
#    Portable to bash 3.2 (macOS): collect (glob \t track) lines, sort, scan dupes.
pairs_file="$(mktemp -t boardlint.XXXXXX)"
trap 'rm -f "$pairs_file"' EXIT

for f in "${contracts[@]+"${contracts[@]}"}"; do
  track="$(basename "$f" .md)"
  in_allowed=0
  while IFS= read -r line; do
    if [[ "$line" =~ ^allowed_paths: ]]; then in_allowed=1; continue; fi
    if [ $in_allowed -eq 1 ]; then
      if [[ "$line" =~ ^[a-z_]+: ]]; then in_allowed=0; continue; fi
      if [[ "$line" =~ ^[[:space:]]*-[[:space:]]*(.+)$ ]]; then
        glob="${BASH_REMATCH[1]}"
        glob="${glob%%#*}"
        glob="${glob%"${glob##*[![:space:]]}"}"
        printf '%s\t%s\n' "$glob" "$track" >> "$pairs_file"
      fi
    fi
  done < "$f"
done

if [ -s "$pairs_file" ]; then
  while IFS=$'\t' read -r glob count first second; do
    [ "$count" -lt 2 ] && continue
    if grep -qF -- "$glob" "$OWNERSHIP_FILE" 2>/dev/null || \
       grep -qF -- "$glob" "$CONFLICT_FILE" 2>/dev/null; then
      continue
    fi
    bail "overlap: '$glob' claimed by multiple tracks ($first, $second...) — not declared multi-owner"
  done < <(
    sort "$pairs_file" | awk -F'\t' '
      { paths[$1] = paths[$1] " " $2; counts[$1]++ }
      END {
        for (p in counts) {
          n = split(paths[p], arr, " ")
          # arr[1] is empty string from leading space; pick arr[2], arr[3]
          printf "%s\t%d\t%s\t%s\n", p, counts[p], arr[2], (counts[p]>1?arr[3]:"")
        }
      }
    '
  )
fi

if [ "$violations" -gt 0 ]; then
  echo
  echo "board-lint: $violations violation(s)"
  exit 1
fi

echo
echo "board-lint: clean"
