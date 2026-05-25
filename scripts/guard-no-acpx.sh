#!/usr/bin/env bash
# Fails if any LIVE acpx / subscription-auth reference exists in the shipped
# surface. Historical tombstones, archived notes, and the dated Cline
# planning/spec docs (which describe the purge itself) are allow-listed.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Patterns that must not appear in live surface. Note: bare `openclaw` is NOT
# included — it collides with an unrelated Mantle "openclaw competition" skill
# reference, and every acpx-context `openclaw` mention lives inside blocks the
# acpx/AcpxIntern patterns already cover.
PATTERN='acpx|AcpxIntern|XVN_INTERN_ACPX'

# Files/dirs allowed to mention ACPX as historical record only (tombstones,
# dated research, ADRs, archived notes, and the dated Cline plan/spec docs).
ALLOW='^docs/cli-non-surfaced.md|^docs/superpowers/notes/|^docs/superpowers/research/|^docs/superpowers/plans/2026-05-24-cline-|^docs/superpowers/specs/2026-05-24-cline-runtime-unification-design.md|^docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md|^decisions/|^team/intake/archive/|^team/archive/|^scripts/guard-no-acpx.sh|^scripts/board-lint.sh'

hits="$(git grep -nIE "$PATTERN" -- \
  ':!target' ':!node_modules' \
  | grep -vE "$ALLOW" || true)"

if [[ -n "$hits" ]]; then
  echo "guard-no-acpx: FAIL — live ACPX references remain:" >&2
  echo "$hits" >&2
  exit 1
fi
echo "guard-no-acpx: OK — no live ACPX references."
