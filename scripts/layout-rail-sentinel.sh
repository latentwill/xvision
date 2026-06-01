#!/usr/bin/env bash
# Scan frontend routes for three-pane layout violations and illegal popup patterns.
# Exits 1 if violations found. Add `# layout-ok` inline to suppress.
#
# Rules enforced:
#   - Routes inside <Layout> (routes.tsx children of the Layout element) must not
#     use right-column grid splits (lg:grid-cols-[..._Npx]).
#   - `fixed inset-0` overlays outside documented exceptions are popup violations.
#
# Documented exceptions (never reported as violations):
#   - frontend/web/src/components/primitives/Toast* (transient feedback)
#   - frontend/web/src/features/marketplace/components/FilterDrawer.tsx (docked, not modal)
#   - frontend/web/src/features/mobile/MListSheet.tsx (mobile list filter, operator-approved)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROUTES_DIR="$REPO_ROOT/frontend/web/src/routes"
FEATURES_DIR="$REPO_ROOT/frontend/web/src/features"

VIOLATIONS=0

echo "=== Layout violations (two-column right sidebar under <Layout>) ==="
results=$(grep -rn \
    -e 'lg:grid-cols-\[.*_[0-9]\+px\]' \
    -e 'xl:grid-cols-\[.*_[0-9]\+px\]' \
    "$ROUTES_DIR" 2>/dev/null | grep -v "layout-ok" || true)
if [ -n "$results" ]; then
    echo "$results" | sed 's/^/  VIOLATION: /'
    VIOLATIONS=$((VIOLATIONS + 1))
else
    echo "  None found."
fi

echo ""
echo "=== Popup violations (fixed inset-0 overlays outside exceptions) ==="
POPUP_EXCEPTIONS=(
    "FilterDrawer.tsx"
    "MListSheet.tsx"
    "Toast"
)

exception_pattern=$(printf "|%s" "${POPUP_EXCEPTIONS[@]}")
exception_pattern="${exception_pattern:1}"

popup_results=$(grep -rn "fixed inset-0" \
    "$ROUTES_DIR" "$FEATURES_DIR" 2>/dev/null \
    | grep -v "layout-ok" \
    | grep -vE "$exception_pattern" || true)

if [ -n "$popup_results" ]; then
    echo "$popup_results" | sed 's/^/  VIOLATION: /'
    VIOLATIONS=$((VIOLATIONS + 1))
else
    echo "  None found."
fi

echo ""
if [ "$VIOLATIONS" -gt 0 ]; then
    echo "Found $VIOLATIONS violation type(s). See CLAUDE.md §'Frontend layout rule' and §'Frontend UI rule: no popups'."
    exit 1
fi

echo "Layout and popup check passed."
