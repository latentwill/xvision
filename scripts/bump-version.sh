#!/usr/bin/env bash
# scripts/bump-version.sh — bump xvision's version across all surfaces.
#
# Usage:
#   scripts/bump-version.sh 0.22.0           # bump and write
#   scripts/bump-version.sh --dry-run 0.22.0 # show diff without writing
#
# What it touches:
#   - Cargo.toml [workspace.package].version  (the source of truth)
#   - frontend/web/package.json .version       (mirror)
#   - Cargo.lock                               (refreshed via `cargo check --workspace`)
#   - CHANGELOG.md                             (prepends an empty [<new>] - <date> section)
#
# What it does NOT do:
#   - git add / commit / tag / push. The operator commits after filling in
#     the new changelog section. The script prints the suggested commands.
#
# Refuses:
#   - non-strict version (must match ^[0-9]+\.[0-9]+\.[0-9]+$)
#   - downgrades (new <= current)
#   - drift between Cargo.toml and frontend/web/package.json at script entry
#
# Versioning scheme: docs/VERSIONING.md.

set -euo pipefail

DRY_RUN=0
NEW_VERSION=""

for arg in "$@"; do
  case "$arg" in
    --dry-run|-n) DRY_RUN=1 ;;
    -h|--help)
      sed -n '2,30p' "$0"
      exit 0
      ;;
    -*)
      echo "bump-version: unknown flag: $arg" >&2
      exit 2
      ;;
    *)
      if [ -n "$NEW_VERSION" ]; then
        echo "bump-version: only one version argument allowed" >&2
        exit 2
      fi
      NEW_VERSION="$arg"
      ;;
  esac
done

if [ -z "$NEW_VERSION" ]; then
  echo "bump-version: missing version argument (e.g. 0.22.0)" >&2
  exit 2
fi

if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "bump-version: '$NEW_VERSION' is not a strict MAJOR.MINOR.PATCH triple" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

CARGO_TOML="Cargo.toml"
WEB_PKG="frontend/web/package.json"
CARGO_LOCK="Cargo.lock"
CHANGELOG="CHANGELOG.md"

for f in "$CARGO_TOML" "$WEB_PKG" "$CHANGELOG"; do
  [ -f "$f" ] || { echo "bump-version: missing $f" >&2; exit 2; }
done

# Extract the current version from Cargo.toml's [workspace.package].
# Portable across BSD awk (macOS) and gawk: isolate the [workspace.package]
# section, take the first version line, grep the triple.
current_cargo="$(
  awk '
    /^\[workspace\.package\]/ { in_section=1; next }
    /^\[/ && in_section       { in_section=0 }
    in_section && /^version[[:space:]]*=/ { print; exit }
  ' "$CARGO_TOML" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1
)"

if [ -z "$current_cargo" ]; then
  echo "bump-version: could not parse current version from $CARGO_TOML" >&2
  exit 2
fi

# Extract from frontend/web/package.json (top-level "version" field).
current_web="$(
  awk -F'"' '/^[[:space:]]*"version"[[:space:]]*:/ { print $4; exit }' "$WEB_PKG"
)"

if [ -z "$current_web" ]; then
  echo "bump-version: could not parse current version from $WEB_PKG" >&2
  exit 2
fi

if [ "$current_cargo" != "$current_web" ]; then
  echo "bump-version: drift detected — $CARGO_TOML is $current_cargo but $WEB_PKG is $current_web." >&2
  echo "Resolve the drift manually before bumping." >&2
  exit 2
fi

CURRENT="$current_cargo"

if [ "$NEW_VERSION" = "$CURRENT" ]; then
  echo "bump-version: new version equals current ($CURRENT) — nothing to do." >&2
  exit 2
fi

# Compare using `sort -V` so 0.10.0 > 0.9.0 etc.
top="$(printf '%s\n%s\n' "$CURRENT" "$NEW_VERSION" | sort -V | tail -1)"
if [ "$top" = "$CURRENT" ]; then
  echo "bump-version: $NEW_VERSION is older than current $CURRENT — refusing to downgrade." >&2
  exit 2
fi

TODAY="$(date -u +%Y-%m-%d)"

echo "bump-version: $CURRENT → $NEW_VERSION (dry-run=$DRY_RUN)"

if [ "$DRY_RUN" -eq 1 ]; then
  echo "  would edit $CARGO_TOML  ([workspace.package].version)"
  echo "  would edit $WEB_PKG    (.version)"
  echo "  would prepend [Unreleased]/[$NEW_VERSION] - $TODAY section to $CHANGELOG"
  echo "  would run: cargo check --workspace  (refresh $CARGO_LOCK)"
  exit 0
fi

# Edit Cargo.toml — only the version line inside [workspace.package].
awk -v new="$NEW_VERSION" '
  BEGIN { in_section=0; done=0 }
  /^\[workspace\.package\]/ { in_section=1; print; next }
  /^\[/ && in_section { in_section=0 }
  in_section && !done && /^version[[:space:]]*=/ {
    sub(/"[0-9]+\.[0-9]+\.[0-9]+"/, "\"" new "\"")
    done=1
  }
  { print }
' "$CARGO_TOML" > "$CARGO_TOML.tmp"
mv "$CARGO_TOML.tmp" "$CARGO_TOML"

# Edit frontend/web/package.json — only the top-level "version" line.
awk -v new="$NEW_VERSION" '
  BEGIN { done=0 }
  !done && /^[[:space:]]*"version"[[:space:]]*:/ {
    sub(/"[0-9]+\.[0-9]+\.[0-9]+"/, "\"" new "\"")
    done=1
  }
  { print }
' "$WEB_PKG" > "$WEB_PKG.tmp"
mv "$WEB_PKG.tmp" "$WEB_PKG"

# Prepend a new section to CHANGELOG.md. Insert after the existing
# "## [Unreleased]" line if it exists; otherwise after the top header.
if grep -q '^## \[Unreleased\]' "$CHANGELOG"; then
  awk -v new="$NEW_VERSION" -v today="$TODAY" '
    BEGIN { inserted=0 }
    /^## \[Unreleased\]/ && !inserted {
      print
      print ""
      print "## [" new "] - " today
      print ""
      print "### Added"
      print ""
      print "### Changed"
      print ""
      print "### Fixed"
      print ""
      inserted=1
      next
    }
    { print }
  ' "$CHANGELOG" > "$CHANGELOG.tmp"
  mv "$CHANGELOG.tmp" "$CHANGELOG"
else
  printf '## [%s] - %s\n\n### Added\n\n### Changed\n\n### Fixed\n\n%s' "$NEW_VERSION" "$TODAY" "$(cat "$CHANGELOG")" > "$CHANGELOG"
fi

# Refresh Cargo.lock so the [[package]] entries for the xvision-* crates
# pick up the new version number. Skip if cargo is not on PATH (deploy
# hosts must never run cargo per CLAUDE.md).
if command -v cargo >/dev/null 2>&1; then
  echo "bump-version: refreshing $CARGO_LOCK via 'cargo check --workspace' …"
  cargo check --workspace >/dev/null 2>&1 || {
    echo "bump-version: cargo check failed; commit what you have and re-run cargo check manually." >&2
  }
else
  echo "bump-version: cargo not on PATH; skipped $CARGO_LOCK refresh." >&2
  echo "  (Deploy hosts intentionally have no Rust toolchain; do not run this script there.)" >&2
fi

cat <<EOF
bump-version: done.

Suggested next steps:
  1. Open $CHANGELOG and fill in the new $NEW_VERSION section.
  2. git add $CARGO_TOML $WEB_PKG $CARGO_LOCK $CHANGELOG
  3. git commit -m "release: $NEW_VERSION"
  4. (Optional) git tag -a v$NEW_VERSION -m "release: $NEW_VERSION"
  5. Push the branch and open a release PR.
  6. After merge, build the image:
       scripts/deploy-image.sh --push <deploy-host>
     and tag the resulting digest as both xvision:deploy-<sha> and
     xvision:$NEW_VERSION.
EOF
