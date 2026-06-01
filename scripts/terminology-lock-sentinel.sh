#!/usr/bin/env bash
# Scan operator-facing surfaces for forbidden cryptographic/internal terms.
# Exits 1 if any violations found. CI-safe; add `# terminology-ok` inline to suppress.
#
# Terminology lock doc: docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

OPERATOR_PATHS=(
    "$REPO_ROOT/MANUAL.md"
    "$REPO_ROOT/crates/xvision-dashboard/wiki"
    "$REPO_ROOT/crates/xvision-cli/src/commands/autooptimizer.rs"
    "$REPO_ROOT/frontend/web/src/features/marketplace"
    "$REPO_ROOT/frontend/web/src/features/memory"
    "$REPO_ROOT/frontend/web/src/features/autooptimizer"
)

FORBIDDEN_PATTERNS=(
    '\bblake3\b'
    '\bmerkle\b'
    '\bed25519\b'
    '\bcanonical.json\b'
    '\bGhost\b'
    '\bQuarantined\b'
    '\bCycleSeal\b'
    '\bMutation\b'
    '\bMutator\b'
    '--gate-epsilon'
    '--parent-holdout-score'
    '--child-holdout-score'
    '--parent-holdout'
    '--child-holdout'
    'gate_epsilon'
    'parent_holdout_score'
    'child_holdout_score'
)

VIOLATIONS=0

for path in "${OPERATOR_PATHS[@]}"; do
    [ -e "$path" ] || continue
    for pattern in "${FORBIDDEN_PATTERNS[@]}"; do
        results=$(grep -rn --include="*.md" --include="*.rs" --include="*.tsx" \
            --include="*.ts" --include="*.txt" \
            "$pattern" "$path" 2>/dev/null | grep -v "terminology-ok" || true)
        if [ -n "$results" ]; then
            echo "VIOLATION [$pattern]:"
            echo "$results" | sed 's/^/  /'
            VIOLATIONS=$((VIOLATIONS + 1))
        fi
    done
done

if [ "$VIOLATIONS" -gt 0 ]; then
    echo ""
    echo "Found $VIOLATIONS terminology violation(s) on operator-facing surfaces."
    echo "See docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md for remediation."
    exit 1
fi

echo "Terminology check passed — no operator-surface leaks found."
